//! Makes a JSON value that fits a JSON Schema, with no model to write it.
//!
//! The same schema and the same seed always give the same value. The recipe
//! follows Pydantic AI's `TestModel`. It reads each schema in this order and
//! takes the first rule that fits:
//!
//! 1. `const` gives that value, `enum` one of its values, and `examples` one
//!    of the examples.
//! 2. `allOf` is merged into one schema, `$ref` is followed inside the same
//!    document, and `anyOf` and `oneOf` give one of their branches.
//! 3. `type` decides the rest. An object gets its required properties only.
//!    A string gets a fixed value for its `format`, or its shortest text. A
//!    number gets its lowest allowed value plus the seed. An array gets its
//!    shortest allowed list, with one item when it may hold any.
//!
//! The seed picks among choices: seed 0 takes the first `enum` value and the
//! first branch, and gives `0`, `false` and `a` where nothing else is said.
//!
//! The recipe reads the keywords above and no others. A `pattern`, and a
//! `oneOf` whose branches overlap, can therefore give a value that does not
//! validate.

use std::cell::Cell;

use serde_json::{Map, Number, Value};

/// How deep the value may nest. A schema that refers to itself would
/// otherwise never end.
const MAX_DEPTH: usize = 12;

/// How many values one call may make, counting every nested one. A schema
/// that asks for a million items would otherwise fill the reply.
const MAX_VALUES: usize = 1000;

/// The most items an array gets, whatever `minItems` asks for.
const MAX_ITEMS: u64 = 64;

/// The longest text a string gets, whatever `minLength` asks for.
const MAX_LENGTH: u64 = 256;

/// The seeds the recipe tells apart. A larger seed counts as its remainder.
/// The items of a list that must differ take the seeds that follow the
/// list's own, and a small span keeps that count from wrapping around.
const SEED_SPAN: u64 = 1 << 32;

/// Makes a value that fits `schema`. `seed` picks among the choices the
/// schema leaves open.
pub(crate) fn generate(schema: &Value, seed: u64) -> Value {
    let generator = Generator {
        root: schema,
        left: Cell::new(MAX_VALUES),
    };
    generator.value(schema, seed % SEED_SPAN, 0)
}

/// One run of the recipe over one schema document.
struct Generator<'a> {
    /// The whole schema, which a `$ref` points into.
    root: &'a Value,
    /// How many more values this run may make.
    left: Cell<usize>,
}

impl Generator<'_> {
    /// The value for `schema`, which sits `depth` levels below the root.
    fn value(&self, schema: &Value, seed: u64, depth: usize) -> Value {
        let left = self.left.get();
        if left == 0 || depth > MAX_DEPTH {
            return Value::Null;
        }
        self.left.set(left - 1);
        let Value::Object(keywords) = schema else {
            // `true` allows any value, and nothing fits `false`.
            return text(seed, 1);
        };
        if let Some(value) = keywords.get("const") {
            return value.clone();
        }
        for list in ["enum", "examples"] {
            if let Some(value) = keywords.get(list).and_then(|values| pick(values, seed)) {
                return value.clone();
            }
        }
        if let Some(Value::Array(parts)) = keywords.get("allOf") {
            return self.value(&self.merged(keywords, parts), seed, depth + 1);
        }
        if let Some(Value::String(pointer)) = keywords.get("$ref") {
            return match self.target(pointer) {
                Some(target) => self.value(target, seed, depth + 1),
                None => text(seed, 1),
            };
        }
        for branches in ["anyOf", "oneOf"] {
            if let Some(branch) = keywords.get(branches).and_then(|values| pick(values, seed)) {
                return self.value(branch, seed, depth + 1);
            }
        }
        match kind(keywords, seed) {
            "object" => self.object(keywords, seed, depth),
            "array" => self.array(keywords, seed, depth),
            "string" => string(keywords, seed),
            "integer" => integer(keywords, seed),
            "number" => number(keywords, seed),
            "boolean" => Value::Bool(seed % 2 == 1),
            "null" => Value::Null,
            _ => text(seed, 1),
        }
    }

    /// Whether this run has made as many values as it may. A list or an
    /// object then stops growing, so the whole value stays small.
    fn spent(&self) -> bool {
        self.left.get() == 0
    }

    /// One schema that holds the keywords of `keywords` and of every member
    /// of its `allOf`. `properties` are joined and `required` lists are
    /// chained. For any other keyword, the member that names it last wins.
    fn merged(&self, keywords: &Map<String, Value>, parts: &[Value]) -> Value {
        let mut merged = keywords.clone();
        merged.remove("allOf");
        for part in parts {
            let part = match part.get("$ref").and_then(Value::as_str) {
                Some(pointer) => self.target(pointer).unwrap_or(part),
                None => part,
            };
            let Value::Object(part) = part else { continue };
            for (name, value) in part {
                match (merged.get_mut(name), value) {
                    (Some(Value::Object(old)), Value::Object(new)) if name == "properties" => {
                        old.extend(new.clone());
                    }
                    (Some(Value::Array(old)), Value::Array(new)) if name == "required" => {
                        old.extend(new.clone());
                    }
                    _ => {
                        merged.insert(name.clone(), value.clone());
                    }
                }
            }
        }
        Value::Object(merged)
    }

    /// The schema that a `$ref` points to. Only a pointer into the same
    /// document is followed, such as `#/$defs/Name`.
    fn target(&self, pointer: &str) -> Option<&Value> {
        self.root.pointer(pointer.strip_prefix('#')?)
    }

    /// An object with its required properties, in the order of their names.
    fn object(&self, keywords: &Map<String, Value>, seed: u64, depth: usize) -> Value {
        let no_properties = Map::new();
        let properties = match keywords.get("properties") {
            Some(Value::Object(properties)) => properties,
            _ => &no_properties,
        };
        let extra = keywords
            .get("additionalProperties")
            .filter(|extra| extra.is_object());
        let mut object = Map::new();
        let required = keywords.get("required").and_then(Value::as_array);
        for name in required.into_iter().flatten().filter_map(Value::as_str) {
            if self.spent() {
                break;
            }
            let any = Value::Bool(true);
            let schema = properties.get(name).or(extra).unwrap_or(&any);
            object.insert(name.to_owned(), self.value(schema, seed, depth + 1));
        }
        // Optional properties join only when the object must be larger.
        let least = keywords
            .get("minProperties")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        for (name, schema) in properties {
            if u64::try_from(object.len()).unwrap_or(u64::MAX) >= least {
                break;
            }
            if !object.contains_key(name) {
                object.insert(name.clone(), self.value(schema, seed, depth + 1));
            }
        }
        // A map from any name to a value gets one entry, so it is not empty.
        if let Some(extra) = extra
            && object.is_empty()
        {
            object.insert(
                "additionalProperty".to_owned(),
                self.value(extra, seed, depth + 1),
            );
        }
        Value::Object(object)
    }

    /// An array of its shortest allowed length, with one item when it may
    /// hold any and names a schema for them.
    fn array(&self, keywords: &Map<String, Value>, seed: u64, depth: usize) -> Value {
        let no_prefix = Vec::new();
        let prefix = match keywords.get("prefixItems") {
            Some(Value::Array(prefix)) => prefix,
            _ => &no_prefix,
        };
        let items = keywords
            .get("items")
            .filter(|items| !matches!(items, Value::Bool(false)));
        let most = keywords
            .get("maxItems")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let least = keywords
            .get("minItems")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let wanted = if prefix.is_empty() && items.is_some() {
            least.max(1)
        } else {
            least.max(u64::try_from(prefix.len()).unwrap_or(u64::MAX))
        };
        let count = wanted.min(most).min(MAX_ITEMS);
        // Items that must differ each get a seed of their own.
        let unique = keywords.get("uniqueItems") == Some(&Value::Bool(true));
        let any = Value::Bool(true);
        let mut array = Vec::new();
        for index in 0..count {
            if self.spent() {
                break;
            }
            let position = usize::try_from(index).unwrap_or(usize::MAX);
            let schema = prefix.get(position).or(items).unwrap_or(&any);
            let seed = if unique {
                seed.wrapping_add(index)
            } else {
                seed
            };
            array.push(self.value(schema, seed, depth + 1));
        }
        Value::Array(array)
    }
}

/// The entry of the list `values` that `seed` picks. `None` when `values`
/// is not a list or is empty.
fn pick(values: &Value, seed: u64) -> Option<&Value> {
    let list = values.as_array()?;
    let length = u64::try_from(list.len())
        .ok()
        .filter(|length| *length > 0)?;
    list.get(usize::try_from(seed % length).ok()?)
}

/// The type the value will have: the `type` keyword, one entry of it when
/// it is a list, or the type that the other keywords imply. Empty when the
/// schema allows any type.
fn kind(keywords: &Map<String, Value>, seed: u64) -> &str {
    match keywords.get("type") {
        Some(Value::String(name)) => name,
        Some(names @ Value::Array(_)) => pick(names, seed).and_then(Value::as_str).unwrap_or(""),
        _ if keywords.contains_key("properties") || keywords.contains_key("required") => "object",
        _ if keywords.contains_key("items") || keywords.contains_key("prefixItems") => "array",
        _ => "",
    }
}

/// A text of `length` letters. The seed picks the letter, so the items of a
/// list that must differ get different texts.
fn text(seed: u64, length: u64) -> Value {
    let letter = char::from(b'a' + u8::try_from(seed % 26).unwrap_or(0));
    let length = usize::try_from(length.min(MAX_LENGTH)).unwrap_or(0);
    Value::String(std::iter::repeat_n(letter, length).collect())
}

/// A string: the fixed value of its `format`, or the shortest text its
/// length bounds allow, which is one letter when they say nothing.
fn string(keywords: &Map<String, Value>, seed: u64) -> Value {
    let format = keywords.get("format").and_then(Value::as_str);
    if let Some(value) = format.and_then(canned) {
        return Value::String(value.to_owned());
    }
    let most = keywords
        .get("maxLength")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    let least = keywords
        .get("minLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    text(seed, least.max(1).min(most))
}

/// The value every string of a known `format` gets.
fn canned(format: &str) -> Option<&'static str> {
    Some(match format {
        "date" => "2024-01-01",
        "date-time" => "2024-01-01T00:00:00Z",
        "time" => "00:00:00Z",
        "duration" => "P1D",
        "email" | "idn-email" => "user@example.com",
        "hostname" | "idn-hostname" => "example.com",
        "ipv4" => "192.0.2.1",
        "ipv6" => "2001:db8::1",
        "uri" | "iri" | "uri-reference" | "iri-reference" => "https://example.com/",
        "uuid" => "00000000-0000-4000-8000-000000000000",
        _ => return None,
    })
}

/// A whole number: the lowest the bounds allow plus the seed, kept inside
/// the bounds, then moved onto a multiple of `multipleOf`.
fn integer(keywords: &Map<String, Value>, seed: u64) -> Value {
    let bound = |inclusive: &str, exclusive: &str, step: i128| {
        whole(keywords.get(inclusive)).or_else(|| Some(whole(keywords.get(exclusive))? + step))
    };
    let low = bound("minimum", "exclusiveMinimum", 1);
    let high = bound("maximum", "exclusiveMaximum", -1);
    let seed = i128::from(seed);
    let mut value = match (low, high) {
        (Some(low), Some(high)) if high >= low => low + seed % (high - low + 1),
        (Some(low), _) => low + seed,
        (None, Some(high)) => high - seed,
        (None, None) => seed,
    };
    if let Some(step) = whole(keywords.get("multipleOf")).filter(|step| *step > 0) {
        value -= value.rem_euclid(step);
        if low.is_some_and(|low| value < low) {
            value += step;
        }
    }
    i64::try_from(value)
        .map(Value::from)
        .or_else(|_| u64::try_from(value).map(Value::from))
        .unwrap_or_else(|_| Value::from(0))
}

/// A bound that is a whole number, as one. A bound with a fraction is not
/// read.
fn whole(bound: Option<&Value>) -> Option<i128> {
    let bound = bound?;
    bound
        .as_i64()
        .map(i128::from)
        .or_else(|| bound.as_u64().map(i128::from))
}

/// A number with a fraction part allowed: the float form of [`integer`].
fn number(keywords: &Map<String, Value>, seed: u64) -> Value {
    let bound = |inclusive: &str, exclusive: &str, step: f64| {
        let read = |name: &str| keywords.get(name).and_then(Value::as_f64);
        read(inclusive).or_else(|| Some(read(exclusive)? + step))
    };
    let low = bound("minimum", "exclusiveMinimum", 1.0);
    let high = bound("maximum", "exclusiveMaximum", -1.0);
    // A seed this small is an exact float.
    let seed = f64::from(u32::try_from(seed % 1000).unwrap_or(0));
    let mut value = match (low, high) {
        (Some(low), Some(high)) if high > low => low + seed % (high - low),
        (Some(low), _) => low + seed,
        (None, Some(high)) => high - seed,
        (None, None) => seed,
    };
    let step = keywords.get("multipleOf").and_then(Value::as_f64);
    if let Some(step) = step.filter(|step| *step > 0.0) {
        value = (value / step).floor() * step;
        if low.is_some_and(|low| value < low) {
            value += step;
        }
    }
    Number::from_f64(value).map_or_else(|| Value::from(0.0), Value::Number)
}

#[cfg(test)]
#[path = "schema-tests.rs"]
mod tests;
