//! One top-level key of a JSON object file under the home directory, whose value is held
//! as JSON text. The other keys of the file stay as they are.
use std::path::PathBuf;

use serde_json::{Map, Value};

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct JsonKey<'a> {
    /// The file, relative to the home directory.
    pub file: &'a str,
    pub key: &'a str,
}

impl JsonKey<'_> {
    fn path(&self, ctx: &Context) -> PathBuf {
        ctx.home.join(self.file)
    }

    /// The file's object, empty when the file is missing.
    fn object(&self, ctx: &Context) -> ModuleResult<Map<String, Value>> {
        let path = self.path(ctx);
        let Ok(text) = std::fs::read(&path) else {
            return Ok(Map::new());
        };
        match serde_json::from_slice(&text)? {
            Value::Object(object) => Ok(object),
            _ => Err(format!("{} does not hold a JSON object", path.display()).into()),
        }
    }

    fn save(&self, ctx: &Context, object: Map<String, Value>) -> ModuleResult {
        let path = self.path(ctx);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec(&Value::Object(object))?)?;
        Ok(())
    }
}

impl Kind for JsonKey<'_> {
    fn describe(&self) -> String {
        format!("{} in ~/{}", self.key, self.file)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        Ok(self.object(ctx)?.get(self.key).map(Value::to_string))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let mut object = self.object(ctx)?;
        object.insert(self.key.to_owned(), serde_json::from_str(value)?);
        self.save(ctx, object)
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let mut object = self.object(ctx)?;
        if object.remove(self.key).is_some() {
            self.save(ctx, object)?;
        }
        Ok(())
    }
}
