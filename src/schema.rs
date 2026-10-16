//! uNabto query model (unabto_queries.xml) and its binary encoding.
//!
//! Requests are the query parameters in declaration order; responses are decoded into the same
//! JSON shape the official client SDK produces. Integers are big-endian, `raw` is a u16 length
//! followed by the bytes, and a list is a u16 count followed by its elements.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    Raw,
}

#[derive(Debug, Clone)]
pub enum Field {
    Param { name: String, kind: Kind },
    List { name: String, fields: Vec<Field> },
}

#[derive(Debug, Clone)]
pub struct Query {
    pub id: u32,
    pub request: Vec<Field>,
    pub response: Vec<Field>,
}

pub struct Schema {
    queries: HashMap<String, Query>,
}

/// One request value, as the callers build it.
#[derive(Debug, Clone)]
pub enum Arg {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<Map<String, Value>>),
}

fn kind(s: &str) -> Result<Kind> {
    Ok(match s {
        "uint8" => Kind::U8,
        "int8" => Kind::I8,
        "uint16" => Kind::U16,
        "int16" => Kind::I16,
        "uint32" => Kind::U32,
        "int32" => Kind::I32,
        "raw" => Kind::Raw,
        other => bail!("unknown parameter type {other}"),
    })
}

fn fields(node: roxmltree::Node) -> Result<Vec<Field>> {
    let mut out = Vec::new();
    for c in node.children().filter(|c| c.is_element()) {
        let name = c
            .attribute("name")
            .ok_or_else(|| anyhow!("field without name"))?
            .to_string();
        match c.tag_name().name() {
            "parameter" => out.push(Field::Param {
                name,
                kind: kind(c.attribute("type").unwrap_or(""))?,
            }),
            "list" => out.push(Field::List {
                name,
                fields: fields(c)?,
            }),
            other => bail!("unexpected element {other}"),
        }
    }
    Ok(out)
}

impl Schema {
    pub fn parse(xml: &str) -> Result<Schema> {
        let doc = roxmltree::Document::parse(xml)?;
        let mut queries = HashMap::new();
        for q in doc
            .root_element()
            .children()
            .filter(|c| c.has_tag_name("query"))
        {
            let name = q
                .attribute("name")
                .ok_or_else(|| anyhow!("query without name"))?;
            let id = q
                .attribute("id")
                .ok_or_else(|| anyhow!("query without id"))?
                .parse()?;
            let child = |tag: &str| q.children().find(|c| c.has_tag_name(tag));
            let request = child("request")
                .map(fields)
                .transpose()?
                .unwrap_or_default();
            let response = child("response")
                .map(fields)
                .transpose()?
                .unwrap_or_default();
            queries.insert(
                name.to_string(),
                Query {
                    id,
                    request,
                    response,
                },
            );
        }
        Ok(Schema { queries })
    }

    pub fn get(&self, name: &str) -> Result<&Query> {
        self.queries
            .get(name)
            .ok_or_else(|| anyhow!("unknown query {name}"))
    }
}

fn put_int(out: &mut Vec<u8>, kind: Kind, n: i64, name: &str) -> Result<()> {
    let (lo, hi) = match kind {
        Kind::U8 => (0, u8::MAX as i64),
        Kind::I8 => (i8::MIN as i64, i8::MAX as i64),
        Kind::U16 => (0, u16::MAX as i64),
        Kind::I16 => (i16::MIN as i64, i16::MAX as i64),
        Kind::U32 => (0, u32::MAX as i64),
        Kind::I32 => (i32::MIN as i64, i32::MAX as i64),
        Kind::Raw => bail!("{name} is raw, got an integer"),
    };
    if n < lo || n > hi {
        bail!("{name}={n} out of range for {kind:?}");
    }
    match kind {
        Kind::U8 | Kind::I8 => out.push(n as u8),
        Kind::U16 | Kind::I16 => out.extend_from_slice(&(n as u16).to_be_bytes()),
        _ => out.extend_from_slice(&(n as u32).to_be_bytes()),
    }
    Ok(())
}

fn put_raw(out: &mut Vec<u8>, bytes: &[u8], name: &str) -> Result<()> {
    let len = u16::try_from(bytes.len()).with_context(|| format!("{name} too long"))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// Values inside request lists come as JSON (numbers or strings).
fn put_json(out: &mut Vec<u8>, field: &Field, v: &Value) -> Result<()> {
    match field {
        Field::Param {
            name,
            kind: Kind::Raw,
        } => put_raw(out, v.as_str().unwrap_or("").as_bytes(), name),
        Field::Param { name, kind } => put_int(
            out,
            *kind,
            v.as_i64()
                .ok_or_else(|| anyhow!("{name} must be a number"))?,
            name,
        ),
        Field::List { name, .. } => bail!("nested list {name} is not supported"),
    }
}

pub fn encode_request(q: &Query, args: &[(&str, Arg)]) -> Result<Vec<u8>> {
    let mut out = q.id.to_be_bytes().to_vec();
    for field in &q.request {
        let (name, arg) = match field {
            Field::Param { name, .. } | Field::List { name, .. } => (
                name,
                args.iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, a)| a)
                    .ok_or_else(|| anyhow!("missing parameter {name}"))?,
            ),
        };
        match (field, arg) {
            (
                Field::Param {
                    kind: Kind::Raw, ..
                },
                Arg::Bytes(b),
            ) => put_raw(&mut out, b, name)?,
            (Field::Param { kind, .. }, Arg::Int(n)) => put_int(&mut out, *kind, *n, name)?,
            (Field::List { fields, .. }, Arg::List(items)) => {
                out.extend_from_slice(&u16::try_from(items.len())?.to_be_bytes());
                for item in items {
                    for f in fields {
                        let fname = match f {
                            Field::Param { name, .. } | Field::List { name, .. } => name,
                        };
                        put_json(&mut out, f, item.get(fname).unwrap_or(&Value::Null))?;
                    }
                }
            }
            _ => bail!("parameter {name} has the wrong type"),
        }
    }
    Ok(out)
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|e| *e <= self.buf.len())
            .ok_or_else(|| anyhow!("response too short"))?;
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
}

fn read_fields(r: &mut Reader, fields: &[Field]) -> Result<Map<String, Value>> {
    let mut obj = Map::new();
    for f in fields {
        match f {
            Field::Param { name, kind } => {
                let v = match kind {
                    Kind::U8 => json!(r.take(1)?[0]),
                    Kind::I8 => json!(r.take(1)?[0] as i8),
                    Kind::U16 => json!(r.u16()?),
                    Kind::I16 => json!(r.u16()? as i16),
                    Kind::U32 => json!(u32::from_be_bytes(r.take(4)?.try_into()?)),
                    Kind::I32 => json!(i32::from_be_bytes(r.take(4)?.try_into()?)),
                    Kind::Raw => {
                        let n = r.u16()? as usize;
                        let b = r.take(n)?;
                        let b = b.strip_suffix(&[0]).unwrap_or(b);
                        json!(String::from_utf8_lossy(b))
                    }
                };
                obj.insert(name.clone(), v);
            }
            Field::List { name, fields } => {
                let n = r.u16()?;
                let mut items = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    items.push(Value::Object(read_fields(r, fields)?));
                }
                obj.insert(name.clone(), Value::Array(items));
            }
        }
    }
    Ok(obj)
}

pub fn decode_response(q: &Query, body: &[u8]) -> Result<Value> {
    let mut r = Reader { buf: body, pos: 0 };
    Ok(Value::Object(read_fields(&mut r, &q.response)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Schema {
        Schema::parse(include_str!("../unabto_queries.xml")).unwrap()
    }

    #[test]
    fn encodes_room_state_like_the_device_expects() {
        let s = schema();
        let q = s.get("room_state.json").unwrap();
        let empty = || Arg::Bytes(vec![]);
        let body = encode_request(
            q,
            &[
                ("id", Arg::Int(0)),
                ("tdelta", Arg::Int(-1)),
                ("vacation", empty()),
                ("season", empty()),
                ("mode", empty()),
                ("name", empty()),
            ],
        )
        .unwrap();
        assert_eq!(body, [0, 0, 1, 1, 0, 0, 0xff, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn decodes_room_list_captured_from_the_device() {
        let s = schema();
        let body = b"\x00\x02\x00\rHabitaci\xc3\xb3n 1\x00\x06living\x00\x0724.8\xc2\xb0C\x00\x0573.4%\x00\x05Ba\xc3\xb1o\x00\x09bathroom1\x00\x0725.2\xc2\xb0C\x00\x02--";
        let v = decode_response(s.get("room_list.json").unwrap(), body).unwrap();
        assert_eq!(v["rooms"][0]["name"], "Habitación 1");
        assert_eq!(v["rooms"][0]["temperature"], "24.8°C");
        assert_eq!(v["rooms"][1]["type"], "bathroom1");
    }

    #[test]
    fn rejects_out_of_range_and_missing() {
        let s = schema();
        let q = s.get("room_timer.json").unwrap();
        assert!(encode_request(q, &[("id", Arg::Int(0))]).is_err());
        let args = [
            ("id", Arg::Int(0)),
            ("type", Arg::Int(0)),
            ("day", Arg::Int(300)),
            ("bf0", Arg::Int(0)),
            ("bf1", Arg::Int(0)),
            ("bf2", Arg::Int(0)),
        ];
        assert!(encode_request(q, &args).is_err());
    }
}
