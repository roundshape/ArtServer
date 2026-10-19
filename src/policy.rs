//! The allowlist of commands passed to the engine (denied by default).
//!
//! Both ArtServer's tools and VectorCraft's tools (through GuardedBackend) pass through here at the engine
//! execution boundary. Commands that change process-wide state or touch the file system are not listed
//! here (docs/design-policy.md sections 4.7 and 6.2).

use serde_json::Value;

/// Who calls a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    /// ArtServer's own tools
    ArtServer,
    /// VectorCraft's tools, including commands the AI names directly through `command_run` and the like
    VectorCraftTool,
}

/// Commands only ArtServer's own tools use. ArtServer manages which formats can be opened or placed and how
/// files are delivered, so VectorCraft's tools may not use them.
const ARTSERVER_ONLY: &[&str] = &["file.new", "document.open", "file.place", "document.export"];

/// Commands both may use.
const SHARED: &[&str] = &[
    // draw_shape / draw_path / set_paint (fill, stroke, dashes, opacity, name, effects)
    "shape.rectangle",
    "shape.ellipse",
    "shape.polygon",
    "shape.star",
    "shape.line",
    "path.create",
    "paint.setFill",
    "paint.setStroke",
    "stroke.set",
    "object.setProps",
    "effect.list",
    "effect.apply",
    // transform
    "object.move",
    "object.rotate",
    "object.scale",
    "object.reflect",
    "object.shear",
    // add_text
    "text.create",
    "text.createInPath",
    "type.pathOptions",
    // pathfinder (select, then combine)
    "select.set",
    "object.pathfinder.unite",
    "object.pathfinder.minusFront",
    "object.pathfinder.intersect",
    "object.pathfinder.exclude",
    "object.pathfinder.divide",
    "object.pathfinder.trim",
    "object.pathfinder.merge",
    "object.pathfinder.crop",
    "object.pathfinder.outline",
    "object.pathfinder.minusBack",
    // create_graph / text_wrap
    "graph.create",
    "object.textWrap.make",
    "object.textWrap.release",
    // Document state, undo and redo
    "document.inspect",
    "document.node",
    "edit.undo",
    "edit.redo",
    // Fonts (list_fonts)
    "text.fontList",
    // From here on, commands for command_run and the like. Editing text
    "text.setText",
    "text.setStyle",
    "text.setFormat",
    "text.setRangeStyle",
    "text.areaOptions",
    "text.reshapeArea",
    "type.createOutlines",
    "type.orientation.vertical",
    "type.orientation.horizontal",
    "type.convertToAreaType",
    "type.convertToPointType",
    // Checking text and fonts (read only)
    "text.getRange",
    "text.fonts",
    "text.missingFonts",
    // Selection, stacking order, alignment
    "select.add",
    "select.none",
    "select.all",
    "object.arrange.bringToFront",
    "object.arrange.bringForward",
    "object.arrange.sendBackward",
    "object.arrange.sendToBack",
    "object.align",
    "object.distribute",
    "object.transform",
    // Groups, deletion, duplication, layers
    "object.group",
    "object.ungroup",
    "edit.clear",
    "edit.duplicate",
    "layer.new",
    "layer.setCurrent",
    "layer.setProps",
    "layer.delete",
    // First batch of the commands that edit only within the document (docs/design-policy.md section 4.7,
    // 2026-10-19). Preferences they set live in the engine's Session, so they stay within the working session.
    // Read only: inspecting the document
    "appearance.newArt",
    "attributes.info",
    "brush.get",
    "brush.list",
    "colorTheme.list",
    "dataset.export", // only as data: a string `path` is refused like every path
    "dataset.list",
    "graphicStyle.list",
    "graphicStyle.setOptions",
    "graphicStyle.unused",
    "guide.list",
    "livePaint.info",
    "object.envelope.info",
    "object.expand.info",
    "paint.freeform.get",
    "paint.proxies",
    "paint.recent",
    "pattern.list",
    "perspective.grid.get",
    "recolor.colors",
    "recolor.randomize",
    "recolor.reduce",
    "select.savedList",
    "shapeBuilder.regions",
    "stroke.widthProfile.list",
    "text.tabs.get",
    "text.thread.info",
    "variable.list",
    // Artboards. artboard.copy and artboard.cut wait for the clipboard commands, decided later
    "artboard.convertToArtboards",
    "artboard.delete",
    "artboard.duplicate",
    "artboard.fitToArt",
    "artboard.fitToSelection",
    "artboard.move",
    "artboard.new",
    "artboard.rearrange",
    "artboard.reorder",
    "artboard.setProps",
    // Transparency and opacity masks. transparency.viewOpacityMask only changes what the screen shows
    "transparency.disableOpacityMask",
    "transparency.editOpacityMask",
    "transparency.enableOpacityMask",
    "transparency.info",
    "transparency.linkOpacityMask",
    "transparency.makeOpacityMask",
    "transparency.opacityMaskInfo",
    "transparency.releaseOpacityMask",
    "transparency.set",
    "transparency.setOpacityMask",
    "transparency.stopEditingOpacityMask",
    "transparency.toggleNewMasksClipping",
    "transparency.toggleNewMasksInverted",
    "transparency.togglePageIsolatedBlending",
    "transparency.togglePageKnockoutGroup",
    "transparency.unlinkOpacityMask",
    // Gradients
    "gradient.selectStop",
    // Appearance
    "appearance.addEffect",
    "appearance.addFill",
    "appearance.addStroke",
    "appearance.clear",
    "appearance.copyFrom",
    "appearance.duplicateItem",
    "appearance.moveItem",
    "appearance.reduceToBasic",
    "appearance.removeItem",
    "appearance.setActiveItem",
    "appearance.setItem",
    "appearance.setNewArtBasic",
    "appearance.showAllHidden",
    "appearance.targetContents",
    "appearance.transfer",
    // Swatches of the document (swatch libraries stay closed)
    "swatch.addUsedColors",
    "swatch.delete",
    "swatch.duplicate",
    "swatch.edit",
    "swatch.editGroup",
    "swatch.list",
    "swatch.merge",
    "swatch.move",
    "swatch.new",
    "swatch.newGroup",
    "swatch.resetDefaults",
    "swatch.setSpot",
    "swatch.sortByKind",
    "swatch.sortByName",
    "swatch.spotOptions",
    "swatch.ungroup",
    "swatch.unused",
    // Symbols
    "symbol.adjust",
    "symbol.breakLink",
    "symbol.delete",
    "symbol.duplicate",
    "symbol.edit",
    "symbol.list",
    "symbol.new",
    "symbol.options",
    "symbol.place",
    "symbol.replace",
    "symbol.selectInstances",
    "symbol.setCurrent",
    "symbol.spray",
    "symbol.update",
    // Clipping masks, compound paths and blends
    "object.clippingMask.editContents",
    "object.clippingMask.editMask",
    "object.clippingMask.make",
    "object.clippingMask.release",
    "object.compoundPath.make",
    "object.compoundPath.release",
    "object.blend.expand",
    "object.blend.info",
    "object.blend.make",
    "object.blend.options",
    "object.blend.release",
    "object.blend.replaceSpine",
    "object.blend.reverseFrontToBack",
    "object.blend.reverseSpine",
    "object.blend.spine.addAnchor",
    "object.blend.spine.convertAnchor",
    "object.blend.spine.moveAnchor",
    "object.blend.spine.removeAnchor",
    "object.blend.spine.reshapeSegment",
];

/// Formats allowed for export.
pub const EXPORT_FORMATS: &[&str] = &["svg", "pdf", "png", "ai", "vectorcraft"];

/// Extensions of the file names that may be opened or placed, always with the file's contents as data. The
/// tools check the contents too (src/uploads.rs), since the engine tells formats from the contents.
const IMPORT_EXTENSIONS: &[&str] = &["svg", "pdf", "ai", "png", "jpg", "vectorcraft"];

/// Arguments that may name a place in the file system. Refused when the value is a string. Ones that take a
/// shape id (a number), such as `path` of `text.createInPath`, pass.
const PATH_KEYS: &[&str] = &["path", "paths", "folder", "destination", "destinationFolder", "file", "files"];

/// May `caller` use `command`?
pub fn is_allowed(command: &str, caller: Caller) -> bool {
    SHARED.contains(&command) || (caller == Caller::ArtServer && ARTSERVER_ONLY.contains(&command))
}

/// Every command on the allowlist.
pub fn allowed_commands() -> impl Iterator<Item = &'static str> {
    ARTSERVER_ONLY.iter().chain(SHARED).copied()
}

/// May `caller` run `command` with `params`? If not, returns the reason.
pub fn check(command: &str, params: &Value, caller: Caller) -> Result<(), String> {
    if !is_allowed(command, caller) {
        return Err(format!("command `{command}` is not allowed"));
    }
    let Value::Object(map) = params else {
        return Err(format!("parameters of `{command}` must be an object"));
    };
    if let Some(key) = PATH_KEYS.iter().find(|k| map.get(**k).is_some_and(names_a_file)) {
        return Err(format!("parameter `{key}` is not allowed: this server does not accept file paths"));
    }
    match command {
        "document.export" => {
            let format = map.get("format").and_then(Value::as_str);
            if !format.is_some_and(|f| EXPORT_FORMATS.contains(&f)) {
                return Err(format!("export format must be one of {}", EXPORT_FORMATS.join(", ")));
            }
            // .ai files are written PDF-compatible only, never in the legacy PostScript form (docs/design-policy.md section 4.6)
            if map.get("aiVersion").is_some_and(|v| v != "pdf") {
                return Err("only PDF-compatible .ai files are written".to_string());
            }
        }
        "document.open" | "file.place" => {
            let name = map.get("name").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
            let extension = name.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
            if !map.contains_key("dataBase64") || !IMPORT_EXTENSIONS.contains(&extension) {
                return Err(format!(
                    "only data (`name` and `dataBase64`) in these formats may be opened or placed: {}",
                    IMPORT_EXTENSIONS.join(", ")
                ));
            }
            if map.get("link").is_some_and(|l| l != &Value::Bool(false)) {
                return Err("linked files are not available on this server; files are always embedded".to_string());
            }
        }
        // Effects that run installed plug-ins (WebAssembly) are not allowed
        "effect.apply" if map.get("effect").and_then(Value::as_str).is_some_and(|e| e.starts_with("plugin.")) => {
            return Err("plug-in effects are not available on this server".to_string());
        }
        _ => {}
    }
    Ok(())
}

/// Can the value be read as a file location (a string, or an array containing strings)?
fn names_a_file(value: &Value) -> bool {
    match value {
        Value::String(_) => true,
        Value::Array(items) => items.iter().any(names_a_file),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BOTH: [Caller; 2] = [Caller::ArtServer, Caller::VectorCraftTool];

    #[test]
    fn commands_outside_the_list_are_refused() {
        for id in [
            "plugin.install",
            "plugin.run",
            "color.loadProfile",
            "prefs.set",
            "command.batch",
            "action.batch",
            "file.saveAs",
            "help.website",
            "text.addFontFiles",
            // Left out of the first batch of document commands (docs/design-policy.md section 4.7)
            "artboard.copy",
            "artboard.cut",
            "graphicStyle.libraries",
            "graphicStyle.library",
            "perspective.presets.import",
            "perspective.presets.export",
            "perspective.widget.options",
            "transparency.viewOpacityMask",
        ] {
            for caller in BOTH {
                assert!(check(id, &json!({}), caller).is_err(), "{id} should be refused");
            }
        }
        assert!(
            check(
                "shape.rectangle",
                &json!({"x": 0, "y": 0, "width": 1, "height": 1}),
                Caller::VectorCraftTool
            )
            .is_ok()
        );
    }

    #[test]
    fn opening_and_exporting_are_left_to_artserver_tools() {
        for id in ARTSERVER_ONLY {
            assert!(!is_allowed(id, Caller::VectorCraftTool), "{id}");
            assert!(is_allowed(id, Caller::ArtServer), "{id}");
        }
        assert!(check("document.export", &json!({"format": "svg"}), Caller::VectorCraftTool).is_err());
    }

    #[test]
    fn file_paths_are_refused_but_object_ids_pass() {
        assert!(check("document.export", &json!({"format": "svg", "path": "/tmp/x.svg"}), Caller::ArtServer).is_err());
        assert!(check("document.open", &json!({"path": "/etc/hosts"}), Caller::ArtServer).is_err());
        assert!(check("shape.rectangle", &json!({"file": "x"}), Caller::VectorCraftTool).is_err());
        assert!(check("text.createInPath", &json!({"path": ["/etc/hosts"]}), Caller::VectorCraftTool).is_err());
        // dataset.export writes a file only when given a path; as data it is open
        assert!(check("dataset.export", &json!({"path": "/tmp/data.csv"}), Caller::VectorCraftTool).is_err());
        assert!(check("dataset.export", &json!({}), Caller::VectorCraftTool).is_ok());
        assert!(
            check(
                "text.createInPath",
                &json!({"path": 3, "mode": "onPath", "text": "a"}),
                Caller::VectorCraftTool
            )
            .is_ok()
        );
    }

    #[test]
    fn export_open_and_place_are_limited_to_known_forms() {
        assert!(check("document.export", &json!({"format": "eps"}), Caller::ArtServer).is_err());
        assert!(check("document.export", &json!({}), Caller::ArtServer).is_err());
        for format in ["svg", "pdf", "png", "ai", "vectorcraft"] {
            assert!(
                check("document.export", &json!({"format": format}), Caller::ArtServer).is_ok(),
                "{format}"
            );
        }
        assert!(check("document.export", &json!({"format": "ai", "aiVersion": "pdf"}), Caller::ArtServer).is_ok());
        assert!(check("document.export", &json!({"format": "ai", "aiVersion": "7"}), Caller::ArtServer).is_err());
        assert!(check("document.export", &json!({"format": "ai", "aiVersion": 7}), Caller::ArtServer).is_err());
        for id in ["document.open", "file.place"] {
            for name in ["a.SVG", "a.pdf", "a.ai", "a.png", "a.jpg", "a.vectorcraft"] {
                assert!(
                    check(id, &json!({"name": name, "dataBase64": ""}), Caller::ArtServer).is_ok(),
                    "{id} {name}"
                );
            }
            for name in ["a.eps", "a.psd", "a.vctemplate", "a.svgz", "svg"] {
                assert!(
                    check(id, &json!({"name": name, "dataBase64": ""}), Caller::ArtServer).is_err(),
                    "{id} {name}"
                );
            }
            assert!(check(id, &json!({"name": "a.png"}), Caller::ArtServer).is_err(), "{id} without data");
            assert!(check(id, &json!({"path": "/tmp/a.png"}), Caller::ArtServer).is_err(), "{id} with a path");
        }
        assert!(check("file.place", &json!({"name": "a.png", "dataBase64": "", "link": true}), Caller::ArtServer).is_err());
        assert!(
            check(
                "file.place",
                &json!({"name": "a.png", "dataBase64": "", "link": false}),
                Caller::ArtServer
            )
            .is_ok()
        );
    }

    #[test]
    fn plug_in_effects_are_refused() {
        assert!(check("effect.apply", &json!({"effect": "plugin.example"}), Caller::VectorCraftTool).is_err());
        assert!(check("effect.apply", &json!({"effect": "stylize.dropShadow"}), Caller::VectorCraftTool).is_ok());
    }

    #[test]
    fn parameters_must_be_an_object() {
        assert!(check("edit.undo", &json!([]), Caller::ArtServer).is_err());
    }

    #[test]
    fn every_allowed_command_exists_in_the_engine() {
        // Notices when a VectorCraft update renames or removes a command
        for id in allowed_commands() {
            assert!(vectorcraft_engine::find_command(id).is_some(), "the engine has no command `{id}`");
        }
    }

    #[test]
    fn closed_namespaces_stay_closed() {
        const CLOSED: &[&str] = &[
            "prefs.",
            "plugin.",
            "script.",
            "help.",
            "shortcutSet.",
            "action.",
            "command.",
            "print.",
            "links.",
            "library.",
            "view.",
            "color.loadProfile",
            "color.removeProfile",
            "text.addFontFiles",
            "text.findFontFiles",
            "text.rescanFonts",
            "edit.colorSettings",
            "edit.assignProfile",
        ];
        for id in allowed_commands() {
            assert!(!CLOSED.iter().any(|c| id.starts_with(c)), "`{id}` belongs to a closed namespace");
            // file.place is open only with data, never a path (see `check`)
            assert!(
                !id.starts_with("file.") || matches!(id, "file.new" | "file.place"),
                "`{id}` touches files"
            );
        }
    }
}
