//! エンジンに渡すコマンドの許可リスト（既定で拒否）。
//!
//! ArtServer のツールも VectorCraft のツール（GuardedBackend 経由）も、エンジン実行境界で
//! ここを通る。プロセス全体の状態を変えるコマンドやファイルシステムに触れるコマンドは
//! ここに載せない（docs/design-policy.md 4.7節、6.2節）。

use serde_json::Value;

/// コマンドを呼ぶ側。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    /// ArtServer 自身のツール
    ArtServer,
    /// VectorCraft のツール。`command_run` などで AI が直接指定したコマンドを含む
    VectorCraftTool,
}

/// ArtServer 自身のツールだけが使うコマンド。読み込める形式と、書き出しの受け渡しを
/// ArtServer で管理するため、VectorCraft のツールからは使わせない。
const ARTSERVER_ONLY: &[&str] = &["file.new", "document.open", "document.export"];

/// どちらからも使えるコマンド。
const SHARED: &[&str] = &[
    // draw_shape / draw_path / set_paint（塗り・線・破線・不透明度・名前・効果）
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
    // pathfinder（選択してから合成する）
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
    // 文書の状態、取り消し・やり直し
    "document.inspect",
    "document.node",
    "edit.undo",
    "edit.redo",
    // フォント（list_fonts）
    "text.fontList",
    // ここから下は command_run などで使えるもの。文字の編集
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
    // 文字とフォントの確認（読み取りだけ）
    "text.getRange",
    "text.fonts",
    "text.missingFonts",
    // 選択・重なり順・整列
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
    // グループ・削除・複製・レイヤー
    "object.group",
    "object.ungroup",
    "edit.clear",
    "edit.duplicate",
    "layer.new",
    "layer.setCurrent",
    "layer.setProps",
    "layer.delete",
];

/// 書き出しを許す形式。
pub const EXPORT_FORMATS: &[&str] = &["svg", "pdf", "png"];

/// ファイルシステムの場所を指しうる引数。値が文字列なら断る。`text.createInPath` の `path`
/// のように図形の id（数値）を取るものは通す。
const PATH_KEYS: &[&str] = &["path", "paths", "folder", "destination", "destinationFolder", "file", "files"];

/// `caller` が `command` を使ってよいか。
pub fn is_allowed(command: &str, caller: Caller) -> bool {
    SHARED.contains(&command) || (caller == Caller::ArtServer && ARTSERVER_ONLY.contains(&command))
}

/// 許可リストのすべてのコマンド。
pub fn allowed_commands() -> impl Iterator<Item = &'static str> {
    ARTSERVER_ONLY.iter().chain(SHARED).copied()
}

/// `caller` が `command` を `params` で実行してよいか。だめなら理由を返す。
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
        }
        "document.open" => {
            let name = map.get("name").and_then(Value::as_str).unwrap_or_default();
            if !map.contains_key("dataBase64") || !name.to_ascii_lowercase().ends_with(".svg") {
                return Err("only SVG data (`name` ending in .svg with `dataBase64`) may be opened".to_string());
            }
        }
        // インストールされたプラグイン（WebAssembly）を動かす効果は使わせない
        "effect.apply" if map.get("effect").and_then(Value::as_str).is_some_and(|e| e.starts_with("plugin.")) => {
            return Err("plug-in effects are not available on this server".to_string());
        }
        _ => {}
    }
    Ok(())
}

/// ファイルの場所として読める値か（文字列、または文字列を含む配列）。
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
    fn export_and_open_are_limited_to_known_forms() {
        assert!(check("document.export", &json!({"format": "eps"}), Caller::ArtServer).is_err());
        assert!(check("document.export", &json!({}), Caller::ArtServer).is_err());
        assert!(check("document.export", &json!({"format": "pdf"}), Caller::ArtServer).is_ok());
        assert!(check("document.open", &json!({"name": "a.pdf", "dataBase64": ""}), Caller::ArtServer).is_err());
        assert!(check("document.open", &json!({"name": "a.SVG", "dataBase64": ""}), Caller::ArtServer).is_ok());
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
        // VectorCraft の更新で名前が変わったり消えたりしたら、ここで気づく
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
            assert!(!id.starts_with("file.") || id == "file.new", "`{id}` touches files");
        }
    }
}
