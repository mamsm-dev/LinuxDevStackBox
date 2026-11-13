//! Shared by the desktop app and the small console PHP launcher. No GUI dependencies.
use std::path::Path;

pub const EXTENSIONS: &[&str] = &[
    "mysqli", "pdo_mysql", "pdo_sqlite", "sqlite3", "mbstring", "openssl", "curl", "gd", "zip",
    "intl", "fileinfo", "exif", "sockets", "sodium", "ctype", "iconv", "xml", "dom", "simplexml",
    "xmlreader", "xmlwriter", "tokenizer", "phar", "posix",
];

/// Distribution PHP builds ship most extensions as shared objects that depend
/// on others being loaded first (normally handled by conf.d priorities, which
/// the app disables). Dependencies are emitted before the extension itself.
fn dependencies(name: &str) -> &'static [&'static str] {
    match name {
        "mysqli" => &["mysqlnd"],
        "pdo_mysql" => &["pdo", "mysqlnd"],
        "pdo_sqlite" => &["pdo"],
        "dom" | "simplexml" | "xmlreader" | "xmlwriter" => &["xml"],
        _ => &[],
    }
}

fn directive(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.starts_with(';') || line.starts_with('#') { return None; }
    line.split_once('=').map(|(key, value)| (key.trim(), value.trim()))
}

fn value(raw: &str) -> &str {
    if raw.starts_with('"') || raw.starts_with('\'') {
        return raw[1..].split(raw.chars().next().unwrap()).next().unwrap_or("");
    }
    raw.split(';').next().unwrap_or("").trim()
}

pub fn needs_default(content: &str, name: &str) -> bool {
    content
        .lines()
        .rfind(|line| directive(line).is_some_and(|(key, _)| key.eq_ignore_ascii_case(name)))
        .and_then(|line| directive(line))
        .is_none_or(|(_, raw)| value(raw).is_empty())
}

/// Fill missing/empty temporary paths, independently of old upload-limit migrations.
/// Nonempty custom paths remain user-owned, even when currently unavailable.
pub fn temporary_defaults(content: &str, tmp: &str) -> String {
    let mut result = content.to_owned();
    for name in ["sys_temp_dir", "upload_tmp_dir"] {
        if !needs_default(&result, name) { continue; }
        let kept = result.lines().filter(|line| !directive(line).is_some_and(|(key, _)| key.eq_ignore_ascii_case(name))).collect::<Vec<_>>().join("\n");
        result = format!("{name} = \"{tmp}\"\n{kept}\n");
    }
    result
}

fn is_active(content: &str, name: &str) -> bool {
    content.lines().filter_map(directive).any(|(key, raw)| {
        let val = value(raw);
        let file = val.rsplit('/').next().unwrap_or("");
        let file = file.strip_suffix(".so").unwrap_or(file);
        key.eq_ignore_ascii_case("extension") && file.eq_ignore_ascii_case(name)
    })
}

/// New profiles only: never re-enable extensions a user disabled in an existing ini.
/// Only extensions whose `<name>.so` exists in `ext` are enabled (built-in
/// extensions need no directive).
pub fn enable_extensions(content: String, ext: &Path) -> String {
    let mut additions = String::new();
    let mut added: Vec<String> = Vec::new();
    let mut add = |name: &str, additions: &mut String| {
        if added.iter().any(|a| a == name)
            || !ext.join(format!("{name}.so")).is_file()
            || is_active(&content, name)
        {
            return;
        }
        added.push(name.to_string());
        additions.push_str(&format!("extension={name}\n"));
    };
    for name in EXTENSIONS {
        if !ext.join(format!("{name}.so")).is_file() {
            continue;
        }
        for dep in dependencies(name) {
            add(dep, &mut additions);
        }
        add(name, &mut additions);
    }
    format!("{additions}{content}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temps_preserve_custom_and_are_idempotent() {
        let source = "; sys_temp_dir =\nupload_tmp_dir = \"/srv/custom uploads\" ; custom\n; phpImportLimitsVersion: 1\n";
        let result = temporary_defaults(source, "/home/test user/tmp");
        assert!(result.contains("sys_temp_dir = \"/home/test user/tmp\""));
        assert!(result.contains("upload_tmp_dir = \"/srv/custom uploads\" ; custom"));
        assert_eq!(result, temporary_defaults(&result, "elsewhere"));
        let blank = temporary_defaults("SYS_TEMP_DIR=\"\" ; unset\nsys_temp_dir=\n", "/tmp/x");
        assert_eq!(blank.matches("sys_temp_dir =").count(), 1);
    }
    #[test]
    fn extension_defaults_match_exact_names_and_quoted_shared_objects() {
        let root = std::env::temp_dir().join(format!("lds-ext-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for name in ["mysqli", "mysqlnd", "pdo", "pdo_sqlite", "sqlite3"] { std::fs::write(root.join(format!("{name}.so")), "").unwrap(); }
        let original = "extension = \"mysqli.so\" ; enabled\n;extension=pdo_sqlite\n";
        let result = enable_extensions(original.into(), &root);
        assert_eq!(result.matches("mysqli.so").count(), 1);
        assert!(result.find("extension=pdo\n").unwrap() < result.find("extension=pdo_sqlite\n").unwrap());
        assert!(result.contains("extension=mysqlnd\n"));
        assert!(result.contains("extension=pdo_sqlite\n"));
        assert!(result.contains("extension=sqlite3\n"));
        assert!(!result.contains("extension=imagick"));
        assert_eq!(result, enable_extensions(result.clone(), &root));
        std::fs::remove_dir_all(root).unwrap();
    }
}
