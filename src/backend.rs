//! GuardedBackend: the gateway through which VectorCraft's tools call the engine (`vectorcraft_mcp::Backend`).
//!
//! VectorCraft's tools call the engine only through this gateway. It runs each call past ArtServer's allowlist
//! and argument checks before executing it, and refuses the gateways for operating the UI and for reading and
//! writing files (docs/design-policy.md section 4.7). It runs on a working session's execution thread.

use serde_json::{Value, json};
use vectorcraft_engine::Session;
use vectorcraft_mcp::Backend;

use crate::engine_host::{EngineLimits, execute_checked};
use crate::policy::{self, Caller};

/// The range of scales allowed when rendering a preview (the same as VectorCraft's renderer).
const RENDER_SCALE: (f64, f64) = (0.01, 16.0);

pub(crate) struct GuardedBackend<'a> {
    session: &'a mut Session,
    limits: &'a EngineLimits,
}

impl<'a> GuardedBackend<'a> {
    pub(crate) fn new(session: &'a mut Session, limits: &'a EngineLimits) -> Self {
        Self { session, limits }
    }

    fn execute(&mut self, command: &str, params: &Value) -> Result<Value, String> {
        execute_checked(self.session, command, params, self.limits, Caller::VectorCraftTool).map_err(|e| e.to_string())
    }

    /// The list of only the commands on the allowlist (the source of `command_list`).
    fn commands(&self) -> Value {
        let allowed = self
            .session
            .commands()
            .into_iter()
            .filter(|c| policy::is_allowed(c.id, Caller::VectorCraftTool))
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        Value::Array(allowed)
    }

    /// Rendering for `render_preview`: one artboard as PNG. It only calls the engine's export with fixed
    /// arguments, so it doesn't touch files. The background is white, as an artboard looks: the engine's
    /// default follows the document and is usually transparent, which a client may show over black, and the
    /// AI may then take the page for black.
    fn render(&mut self, params: &Value) -> Result<Value, String> {
        let scale = params
            .get("scale")
            .and_then(Value::as_f64)
            .unwrap_or(1.0)
            .clamp(RENDER_SCALE.0, RENDER_SCALE.1);
        let artboard = params.get("artboard").and_then(Value::as_u64).unwrap_or(0);
        let out = self
            .session
            .execute(
                "document.export",
                &json!({"format": "png", "scale": scale, "artboard": artboard, "background": "white"}),
            )
            .map_err(|e| e.to_string())?;
        let data = out.get("dataBase64").cloned().ok_or("the renderer returned no image")?;
        Ok(json!({"pngBase64": data}))
    }
}

impl Backend for GuardedBackend<'_> {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "engine.execute" => {
                let command = params.get("command").and_then(Value::as_str).ok_or("missing `command`")?;
                let args = match params.get("params") {
                    None | Some(Value::Null) => json!({}),
                    Some(v) => v.clone(),
                };
                self.execute(command, &args)
            }
            "engine.commands" => Ok(self.commands()),
            "document.inspect" | "document.node" => self.execute(method, &params),
            "ui.render" => self.render(&params),
            other => Err(format!("`{other}` is not available on this server")),
        }
    }

    fn has_ui(&self) -> bool {
        false
    }

    fn describe(&self) -> String {
        "artserver".to_string()
    }
}
