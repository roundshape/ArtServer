//! GuardedBackend: VectorCraft のツールがエンジンを呼ぶための窓口（`vectorcraft_mcp::Backend`）。
//!
//! VectorCraft のツールは、エンジンをすべてこの窓口越しに呼ぶ。ここで ArtServer の許可リストと
//! 引数の検査を通してから実行し、画面の操作やファイルの読み書きの窓口は断る
//! （docs/design-policy.md 4.7節）。作業セッションの実行スレッドの上で動く。

use serde_json::{Value, json};
use vectorcraft_engine::Session;
use vectorcraft_mcp::Backend;

use crate::engine_host::{EngineLimits, execute_checked};
use crate::policy::{self, Caller};

/// プレビューの描画で許す倍率の範囲（VectorCraft の描画と同じ）。
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

    /// 許可リストにあるコマンドだけの一覧（`command_list` の元になる）。
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

    /// `render_preview` のための描画。アートボード1つを PNG にする。決まった引数でエンジンの
    /// 書き出しを呼ぶだけなので、ファイルには触れない。
    fn render(&mut self, params: &Value) -> Result<Value, String> {
        let scale = params
            .get("scale")
            .and_then(Value::as_f64)
            .unwrap_or(1.0)
            .clamp(RENDER_SCALE.0, RENDER_SCALE.1);
        let artboard = params.get("artboard").and_then(Value::as_u64).unwrap_or(0);
        let out = self
            .session
            .execute("document.export", &json!({"format": "png", "scale": scale, "artboard": artboard}))
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
