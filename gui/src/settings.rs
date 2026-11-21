//! ~/.config/inkover/config.toml, edited in place with toml_edit so comments survive.
use std::cell::RefCell;
use std::path::PathBuf;

use toml_edit::{Array, DocumentMut, Item, Value};

pub const DEFAULT_PALETTE: [&str; 9] = [
    "#ee3333", "#26bf4d", "#3380f2", "#ffd91a", "#ff8c1a", "#bf4de6", "#ffffff", "#0d0d0d", "#1ad9e6",
];

pub fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"));
    base.join("inkover/config.toml")
}

pub fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(p),
    }
}

pub struct Settings {
    doc: RefCell<DocumentMut>,
    path: PathBuf,
}

impl Settings {
    pub fn load() -> Self {
        let path = config_path();
        let doc = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.parse::<DocumentMut>().ok())
            .unwrap_or_default();
        Settings { doc: RefCell::new(doc), path }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn f64(&self, key: &str, default: f64) -> f64 {
        let d = self.doc.borrow();
        d.get(key)
            .and_then(|i| i.as_float().or_else(|| i.as_integer().map(|n| n as f64)))
            .unwrap_or(default)
    }

    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.doc.borrow().get(key).and_then(|i| i.as_bool()).unwrap_or(default)
    }

    pub fn str(&self, key: &str, default: &str) -> String {
        self.doc
            .borrow()
            .get(key)
            .and_then(|i| i.as_str())
            .unwrap_or(default)
            .to_string()
    }

    pub fn strings(&self, key: &str) -> Option<Vec<String>> {
        let d = self.doc.borrow();
        let arr = d.get(key)?.as_array()?;
        Some(arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
    }

    pub fn palette(&self) -> Vec<String> {
        let mut p = self
            .strings("palette")
            .unwrap_or_else(|| DEFAULT_PALETTE.iter().map(|s| s.to_string()).collect());
        p.truncate(9);
        p
    }

    fn set_item(&self, key: &str, value: Value) {
        let mut d = self.doc.borrow_mut();
        // Keep any comment decoration on an existing key.
        if let Some(Item::Value(old)) = d.get_mut(key) {
            let decor = old.decor().clone();
            *old = value;
            *old.decor_mut() = decor;
        } else {
            d[key] = Item::Value(value);
        }
    }

    pub fn set_f64(&self, key: &str, v: f64) {
        self.set_item(key, Value::from((v * 1000.0).round() / 1000.0));
    }
    pub fn set_int(&self, key: &str, v: i64) {
        self.set_item(key, Value::from(v));
    }
    pub fn set_bool(&self, key: &str, v: bool) {
        self.set_item(key, Value::from(v));
    }
    pub fn set_str(&self, key: &str, v: &str) {
        self.set_item(key, Value::from(v));
    }
    pub fn set_strings(&self, key: &str, v: &[String]) {
        let mut a = Array::new();
        for s in v {
            a.push(s.as_str());
        }
        self.set_item(key, Value::Array(a));
    }

    /// Write the file. Returns an error string on failure.
    pub fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.path, self.doc.borrow().to_string()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_keep_comments_and_add_missing_keys() {
        let dir = std::env::temp_dir().join(format!("inkover-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "# top comment\nsize = 5.0   # starting size\nnotify = true\n").unwrap();
        let s = Settings { doc: RefCell::new(std::fs::read_to_string(&path).unwrap().parse().unwrap()), path: path.clone() };
        assert_eq!(s.f64("size", 0.0), 5.0);
        s.set_f64("size", 7.5);
        s.set_bool("notify", false);
        s.set_strings("palette", &["#112233".into()]);
        s.set_int("record_fps", 30);
        s.save().unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("# top comment"));
        assert!(out.contains("size = 7.5   # starting size"), "{out}");
        assert!(out.contains("notify = false"));
        assert!(out.contains("palette = [\"#112233\"]"));
        assert!(out.contains("record_fps = 30"));
        assert_eq!(s.palette(), vec!["#112233".to_string()]);
        std::fs::remove_dir_all(dir).ok();
    }
}
