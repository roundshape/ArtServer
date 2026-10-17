//! エンジンに渡すコマンドの許可リスト（既定で拒否）。
//!
//! ツール層は決まったコマンドしか組み立てないが、エンジン実行境界でもう一度検査する。
//! プロセス全体の状態を変えるコマンドやファイルシステムに触れるコマンドはここに載せない
//! （docs/design-policy.md 6.2節）。

use serde_json::Value;

/// 実行を許すコマンド。
const ALLOWED: &[&str] = &[
    "file.new",
    "document.open",
    "document.inspect",
    "document.export",
    "shape.rectangle",
    "shape.ellipse",
    "shape.polygon",
    "shape.star",
    "shape.line",
    "paint.setFill",
    "paint.setStroke",
    "stroke.set",
    "object.move",
    "edit.undo",
    "edit.redo",
];

/// 書き出しを許す形式。
pub const EXPORT_FORMATS: &[&str] = &["svg", "pdf", "png"];

/// ファイルシステムの場所を指す引数。MVP ではどのコマンドにも渡さない。
const PATH_KEYS: &[&str] = &["path", "paths", "folder", "destination", "destinationFolder", "file", "files"];

/// `command` を `params` で実行してよいか。だめなら理由を返す。
pub fn check(command: &str, params: &Value) -> Result<(), String> {
    if !ALLOWED.contains(&command) {
        return Err(format!("command `{command}` is not allowed"));
    }
    let Value::Object(map) = params else {
        return Err(format!("parameters of `{command}` must be an object"));
    };
    if let Some(key) = PATH_KEYS.iter().find(|k| map.contains_key(**k)) {
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
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn commands_outside_the_list_are_refused() {
        for id in [
            "plugin.install",
            "color.loadProfile",
            "prefs.set",
            "command.batch",
            "file.saveAs",
            "help.website",
        ] {
            assert!(check(id, &json!({})).is_err(), "{id} should be refused");
        }
        assert!(check("shape.rectangle", &json!({"x": 0, "y": 0, "width": 1, "height": 1})).is_ok());
    }

    #[test]
    fn path_arguments_are_refused_everywhere() {
        assert!(check("document.export", &json!({"format": "svg", "path": "/tmp/x.svg"})).is_err());
        assert!(check("document.open", &json!({"path": "/etc/hosts"})).is_err());
        assert!(check("shape.rectangle", &json!({"file": "x"})).is_err());
    }

    #[test]
    fn export_and_open_are_limited_to_known_forms() {
        assert!(check("document.export", &json!({"format": "eps"})).is_err());
        assert!(check("document.export", &json!({})).is_err());
        assert!(check("document.export", &json!({"format": "pdf"})).is_ok());
        assert!(check("document.open", &json!({"name": "a.pdf", "dataBase64": ""})).is_err());
        assert!(check("document.open", &json!({"name": "a.SVG", "dataBase64": ""})).is_ok());
    }

    #[test]
    fn parameters_must_be_an_object() {
        assert!(check("edit.undo", &json!([])).is_err());
    }
}
