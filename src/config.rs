//! A smasher file next to the image, or the file named by `-c`.
//!
//! `smasher.json` is read by the parser below. `smasher.yaml` and
//! `smasher.yml` use the same job list in block form. Neither parser is a
//! crate. The shapes are the ones this program writes jobs from, and nothing
//! else.
//!
//! The file is a list, or an object whose only field is `jobs`. Each item is
//! one pass over the image, top to bottom. A missing field stays at the
//! program default. `strip` defaults to `safe`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::Strip;
use crate::error::SmashError;

/// One pass from a smasher file. Empty fields are left to the program default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// `safe` or `all`, when the job sets it.
    pub strip: Option<Strip>,
    /// New width, when the job sets it.
    pub width: Option<u32>,
    /// New height, when the job sets it.
    pub height: Option<u32>,
    /// JPEG quality, when the job sets it.
    pub quality: Option<u8>,
    /// Byte cap as typed, when the job sets it.
    pub bytes: Option<String>,
}

/// Read a `.json`, `.yaml`, or `.yml` smasher file.
///
/// # Errors
///
/// The file is missing, the text is not that format, or a job field is wrong.
pub fn load(path: &Path) -> Result<Vec<Spec>, SmashError> {
    let text = fs::read_to_string(path).map_err(|error| {
        SmashError::message(format!("could not read {}: {error}", path.display()))
    })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let kind = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    let parsed = match kind.as_deref() {
        Some("json") => parse_json(text),
        Some("yaml" | "yml") => parse_yaml(text),
        _ => Err("a smasher file ends in .json or .yaml".to_owned()),
    };
    let value =
        parsed.map_err(|error| SmashError::message(format!("{}: {error}", path.display())))?;
    specs(value).map_err(|error| SmashError::message(format!("{}: {error}", path.display())))
}

/// `smasher.json`, then `smasher.yaml`, then `smasher.yml`, in the image's directory.
///
/// # Errors
///
/// More than one of those files is present.
pub fn beside(image: &Path) -> Result<Option<PathBuf>, SmashError> {
    let dir = image
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let found: Vec<PathBuf> = ["smasher.json", "smasher.yaml", "smasher.yml"]
        .into_iter()
        .map(|name| dir.join(name))
        .filter(|path| path.is_file())
        .collect();
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.into_iter().next()),
        _ => Err(SmashError::message(format!(
            "found more than one smasher file next to {}. Pass -c to pick one.",
            image.display()
        ))),
    }
}

#[derive(Debug, PartialEq)]
enum Value {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

fn specs(value: Value) -> Result<Vec<Spec>, String> {
    let jobs = match value {
        Value::Array(items) => items,
        Value::Object(pairs) => jobs_field(pairs)?,
        _ => return Err("the smasher file needs a jobs list".to_owned()),
    };
    if jobs.is_empty() {
        return Err("the smasher file has no jobs".to_owned());
    }
    jobs.into_iter()
        .enumerate()
        .map(|(index, value)| one_job(index, value))
        .collect()
}

fn jobs_field(pairs: Vec<(String, Value)>) -> Result<Vec<Value>, String> {
    if pairs.len() != 1 || pairs[0].0 != "jobs" {
        return Err("the smasher file needs a jobs list".to_owned());
    }
    match pairs.into_iter().next().map(|(_, value)| value) {
        Some(Value::Array(items)) => Ok(items),
        _ => Err("jobs must be a list".to_owned()),
    }
}

fn one_job(index: usize, value: Value) -> Result<Spec, String> {
    let Value::Object(pairs) = value else {
        return Err(format!("job {} must be a set of fields", index + 1));
    };
    let mut spec = Spec {
        strip: None,
        width: None,
        height: None,
        quality: None,
        bytes: None,
    };
    let mut seen = Vec::new();
    for (key, value) in pairs {
        if seen.iter().any(|existing: &String| existing == &key) {
            return Err(format!("job {} has a duplicate field `{key}`", index + 1));
        }
        seen.push(key.clone());
        if value == Value::Null {
            continue;
        }
        match key.as_str() {
            "strip" => spec.strip = Some(strip_value(index, value)?),
            "width" => spec.width = Some(whole(index, "width", value, 1, 65_535)?),
            "height" => spec.height = Some(whole(index, "height", value, 1, 65_535)?),
            "quality" => {
                spec.quality = Some(
                    u8::try_from(whole(index, "quality", value, 1, 100)?).map_err(|_| {
                        format!("job {} quality needs a number from 1 to 100", index + 1)
                    })?,
                );
            }
            "bytes" => spec.bytes = Some(bytes_value(index, value)?),
            other => {
                return Err(format!("job {} has an unknown field `{other}`", index + 1));
            }
        }
    }
    if spec.quality.is_some() && spec.bytes.is_some() {
        return Err(format!(
            "job {} sets quality and bytes. Use one of them.",
            index + 1
        ));
    }
    Ok(spec)
}

fn strip_value(index: usize, value: Value) -> Result<Strip, String> {
    let Value::String(text) = value else {
        return Err(format!("job {} strip is safe or all", index + 1));
    };
    match text.to_ascii_lowercase().as_str() {
        "safe" => Ok(Strip::Safe),
        "all" => Ok(Strip::All),
        _ => Err(format!("job {} strip is safe or all", index + 1)),
    }
}

fn whole(index: usize, field: &str, value: Value, low: u64, high: u64) -> Result<u32, String> {
    let number = match value {
        Value::Number(number) => number,
        Value::String(text)
            if text.bytes().all(|byte| byte.is_ascii_digit()) && !text.is_empty() =>
        {
            text.parse::<u64>()
                .map_err(|_| format!("job {} {field} is too large", index + 1))?
        }
        _ => return Err(format!("job {} {field} needs a whole number", index + 1)),
    };
    if !(low..=high).contains(&number) {
        return Err(format!(
            "job {} {field} needs a number from {low} to {high}",
            index + 1
        ));
    }
    u32::try_from(number).map_err(|_| format!("job {} {field} is too large", index + 1))
}

fn bytes_value(index: usize, value: Value) -> Result<String, String> {
    match value {
        Value::String(text) if !text.is_empty() => Ok(text),
        _ => Err(format!(
            "job {} bytes is a size like 40kb or 1mb",
            index + 1
        )),
    }
}

struct Cursor<'a> {
    text: &'a str,
    i: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.i..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let next = self.peek()?;
        self.i += next.len_utf8();
        Some(next)
    }

    fn skip(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.bump();
        }
    }

    fn eat(&mut self, expected: char) -> Result<(), String> {
        self.skip();
        if self.bump() == Some(expected) {
            Ok(())
        } else {
            Err(format!("expected `{expected}`"))
        }
    }
}

fn parse_json(text: &str) -> Result<Value, String> {
    let mut cursor = Cursor { text, i: 0 };
    let value = json_value(&mut cursor)?;
    cursor.skip();
    if cursor.i != cursor.text.len() {
        return Err("extra text after the json value".to_owned());
    }
    Ok(value)
}

fn json_value(cursor: &mut Cursor<'_>) -> Result<Value, String> {
    cursor.skip();
    match cursor.peek() {
        Some('{') => json_object(cursor),
        Some('[') => json_array(cursor),
        Some('"') => Ok(Value::String(json_string(cursor)?)),
        Some('t') => json_literal(cursor, "true", Value::Bool(true)),
        Some('f') => json_literal(cursor, "false", Value::Bool(false)),
        Some('n') => json_literal(cursor, "null", Value::Null),
        Some('-' | '0'..='9') => json_number(cursor),
        _ => Err("expected a json value".to_owned()),
    }
}

fn json_literal(cursor: &mut Cursor<'_>, word: &str, value: Value) -> Result<Value, String> {
    if cursor.rest().starts_with(word) {
        cursor.i += word.len();
        Ok(value)
    } else {
        Err(format!("expected {word}"))
    }
}

fn json_object(cursor: &mut Cursor<'_>) -> Result<Value, String> {
    cursor.eat('{')?;
    let mut pairs = Vec::new();
    cursor.skip();
    if cursor.peek() == Some('}') {
        cursor.bump();
        return Ok(Value::Object(pairs));
    }
    loop {
        cursor.skip();
        if cursor.peek() != Some('"') {
            return Err("expected a json field name".to_owned());
        }
        let key = json_string(cursor)?;
        cursor.eat(':')?;
        let value = json_value(cursor)?;
        if pairs.iter().any(|(existing, _)| existing == &key) {
            return Err(format!("duplicate field `{key}`"));
        }
        pairs.push((key, value));
        cursor.skip();
        match cursor.bump() {
            Some(',') => {}
            Some('}') => return Ok(Value::Object(pairs)),
            _ => return Err("expected `,` or `}`".to_owned()),
        }
    }
}

fn json_array(cursor: &mut Cursor<'_>) -> Result<Value, String> {
    cursor.eat('[')?;
    let mut items = Vec::new();
    cursor.skip();
    if cursor.peek() == Some(']') {
        cursor.bump();
        return Ok(Value::Array(items));
    }
    loop {
        items.push(json_value(cursor)?);
        cursor.skip();
        match cursor.bump() {
            Some(',') => {}
            Some(']') => return Ok(Value::Array(items)),
            _ => return Err("expected `,` or `]`".to_owned()),
        }
    }
}

fn json_string(cursor: &mut Cursor<'_>) -> Result<String, String> {
    cursor.eat('"')?;
    let mut out = String::new();
    loop {
        match cursor.bump() {
            Some('"') => return Ok(out),
            Some('\\') => out.push(json_escape(cursor)?),
            Some(c) => out.push(c),
            None => return Err("the json string is not closed".to_owned()),
        }
    }
}

fn json_escape(cursor: &mut Cursor<'_>) -> Result<char, String> {
    match cursor.bump() {
        Some('"') => Ok('"'),
        Some('\\') => Ok('\\'),
        Some('/') => Ok('/'),
        Some('b') => Ok('\u{0008}'),
        Some('f') => Ok('\u{000c}'),
        Some('n') => Ok('\n'),
        Some('r') => Ok('\r'),
        Some('t') => Ok('\t'),
        Some('u') => json_unicode(cursor),
        _ => Err("bad json escape".to_owned()),
    }
}

fn json_unicode(cursor: &mut Cursor<'_>) -> Result<char, String> {
    let rest = cursor.rest();
    if rest.len() < 4 {
        return Err("short json unicode escape".to_owned());
    }
    let hex = &rest[..4];
    cursor.i += 4;
    let code = u32::from_str_radix(hex, 16).map_err(|_| "bad json unicode escape".to_owned())?;
    char::from_u32(code).ok_or_else(|| "bad json unicode escape".to_owned())
}

fn json_number(cursor: &mut Cursor<'_>) -> Result<Value, String> {
    if cursor.peek() == Some('-') {
        return Err("use a positive number".to_owned());
    }
    let start = cursor.i;
    if cursor.bump() == Some('0') && cursor.peek().is_some_and(|c| c.is_ascii_digit()) {
        return Err("json numbers do not start with zero".to_owned());
    }
    while cursor.peek().is_some_and(|c| c.is_ascii_digit()) {
        cursor.bump();
    }
    if cursor.peek().is_some_and(|c| matches!(c, '.' | 'e' | 'E')) {
        return Err("use a whole number".to_owned());
    }
    let digits = &cursor.text[start..cursor.i];
    let number = digits
        .parse::<u64>()
        .map_err(|_| "that number is too large".to_owned())?;
    Ok(Value::Number(number))
}

struct YamlLine {
    indent: usize,
    text: String,
}

fn parse_yaml(text: &str) -> Result<Value, String> {
    let lines = yaml_lines(text)?;
    if lines.is_empty() {
        return Err("the yaml file is empty".to_owned());
    }
    let mut index = 0;
    let value = yaml_node(&lines, &mut index)?;
    if index != lines.len() {
        return Err("extra text after the yaml value".to_owned());
    }
    Ok(value)
}

fn yaml_lines(text: &str) -> Result<Vec<YamlLine>, String> {
    let mut lines = Vec::new();
    for (number, raw) in text.lines().enumerate() {
        if raw.contains('\t') {
            return Err(format!(
                "line {}: smasher yaml uses spaces, not tabs",
                number + 1
            ));
        }
        let trimmed = strip_yaml_comment(raw).trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        if trimmed.trim() == "---" {
            continue;
        }
        let indent = trimmed.chars().take_while(|c| *c == ' ').count();
        let text = trimmed[indent..].trim().to_owned();
        if text.starts_with('{')
            || text.starts_with('[')
            || text.contains(": [")
            || text.contains(": {")
        {
            return Err(format!(
                "line {}: use a block list, not an inline collection",
                number + 1
            ));
        }
        lines.push(YamlLine { indent, text });
    }
    Ok(lines)
}

fn strip_yaml_comment(line: &str) -> &str {
    let mut single = false;
    let mut double = false;
    for (index, c) in line.char_indices() {
        match c {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '#' if !single && !double => return &line[..index],
            _ => {}
        }
    }
    line
}

fn yaml_node(lines: &[YamlLine], index: &mut usize) -> Result<Value, String> {
    let line = lines
        .get(*index)
        .ok_or_else(|| "expected a yaml value".to_owned())?;
    if line.text.starts_with('-') {
        yaml_seq(lines, index, line.indent)
    } else {
        yaml_map(lines, index, line.indent)
    }
}

fn yaml_seq(lines: &[YamlLine], index: &mut usize, indent: usize) -> Result<Value, String> {
    let mut items = Vec::new();
    while let Some(line) = lines.get(*index) {
        if line.indent != indent || !line.text.starts_with('-') {
            break;
        }
        let rest = line.text[1..].trim();
        *index += 1;
        if rest.is_empty() {
            items.push(yaml_node(lines, index)?);
        } else if let Some((key, value)) = yaml_pair(rest)? {
            let mut pairs = vec![inline_field(key, value)?];
            while let Some(next) = lines.get(*index) {
                if next.indent <= indent || next.text.starts_with('-') {
                    break;
                }
                let Some((key, value)) = yaml_pair(&next.text)? else {
                    return Err("expected a field under the list item".to_owned());
                };
                pairs.push(inline_field(key, value)?);
                *index += 1;
            }
            items.push(Value::Object(pairs));
        } else {
            items.push(yaml_scalar(rest)?);
        }
    }
    if items.is_empty() {
        return Err("expected a yaml list".to_owned());
    }
    Ok(Value::Array(items))
}

fn yaml_map(lines: &[YamlLine], index: &mut usize, indent: usize) -> Result<Value, String> {
    let mut pairs = Vec::new();
    while let Some(line) = lines.get(*index) {
        if line.indent != indent || line.text.starts_with('-') {
            break;
        }
        let Some((key, inline)) = yaml_pair(&line.text)? else {
            return Err("expected a field name".to_owned());
        };
        *index += 1;
        let value = if let Some(inline) = inline {
            inline
        } else {
            let next = lines
                .get(*index)
                .ok_or_else(|| format!("`{key}` needs a value"))?;
            if next.indent <= indent {
                return Err(format!("`{key}` needs a nested value"));
            }
            yaml_node(lines, index)?
        };
        if pairs.iter().any(|(existing, _)| existing == &key) {
            return Err(format!("duplicate field `{key}`"));
        }
        pairs.push((key, value));
    }
    if pairs.is_empty() {
        return Err("expected yaml fields".to_owned());
    }
    Ok(Value::Object(pairs))
}

fn inline_field(key: String, value: Option<Value>) -> Result<(String, Value), String> {
    match value {
        Some(value) => Ok((key, value)),
        None => Err(format!("`{key}` needs a value on the same line")),
    }
}

/// `None` when the line is not a `key: value` field. The value is `None` when it is nested.
fn yaml_pair(text: &str) -> Result<Option<(String, Option<Value>)>, String> {
    let Some(split) = text.find(':') else {
        return Ok(None);
    };
    let key = text[..split].trim();
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        return Err(format!("bad field name in `{text}`"));
    }
    let rest = text[split + 1..].trim();
    if rest.is_empty() {
        return Ok(Some((key.to_owned(), None)));
    }
    Ok(Some((key.to_owned(), Some(yaml_scalar(rest)?))))
}

fn yaml_scalar(text: &str) -> Result<Value, String> {
    if let Some(quoted) = text.strip_prefix('"') {
        let inner = quoted
            .strip_suffix('"')
            .ok_or_else(|| "the quote is not closed".to_owned())?;
        return Ok(Value::String(yaml_double(inner)?));
    }
    if let Some(quoted) = text.strip_prefix('\'') {
        let inner = quoted
            .strip_suffix('\'')
            .ok_or_else(|| "the quote is not closed".to_owned())?;
        return Ok(Value::String(inner.replace("''", "'")));
    }
    match text {
        "null" | "~" => Ok(Value::Null),
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        _ if is_plain_number(text) => text
            .parse::<u64>()
            .map(Value::Number)
            .map_err(|_| "that number is too large".to_owned()),
        _ => Ok(Value::String(text.to_owned())),
    }
}

fn is_plain_number(text: &str) -> bool {
    let Some(first) = text.as_bytes().first() else {
        return false;
    };
    if *first == b'0' {
        return text.len() == 1;
    }
    text.bytes().all(|byte| byte.is_ascii_digit())
}

fn yaml_double(text: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            _ => return Err("bad yaml escape".to_owned()),
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{parse_json, parse_yaml, specs};
    use crate::cli::Strip;

    #[test]
    fn json_and_yaml_read_the_same_jobs_top_to_bottom() {
        let json = specs(
            parse_json(r#"{"jobs":[{"width":512},{"strip":"all","quality":40},{"bytes":"40kb"}]}"#)
                .unwrap(),
        )
        .unwrap();
        let yaml = specs(
            parse_yaml("jobs:\n  - width: 512\n  - strip: all\n    quality: 40\n  - bytes: 40kb\n")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(json, yaml);
        assert_eq!(json[0].width, Some(512));
        assert_eq!(json[1].strip, Some(Strip::All));
        assert_eq!(json[1].quality, Some(40));
        assert_eq!(json[2].bytes.as_deref(), Some("40kb"));
    }

    #[test]
    fn a_bare_list_is_the_job_list() {
        let jobs = specs(parse_json(r#"[{"height":128}]"#).unwrap()).unwrap();
        assert_eq!(jobs[0].height, Some(128));
        assert!(jobs[0].strip.is_none());
    }
}
