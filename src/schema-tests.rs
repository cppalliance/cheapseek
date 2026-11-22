use serde_json::{Value, json};

use super::*;

/// Generates a value for `schema` at `seed`, and fails the test unless the
/// `jsonschema` crate says the value fits, formats included.
fn valid_at(schema: &Value, seed: u64) -> Value {
    let value = generate(schema, seed);
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(schema)
        .expect("a schema the validator accepts");
    if let Err(error) = validator.validate(&value) {
        panic!("{value} does not fit {schema}: {error}");
    }
    value
}

/// The valid value for `schema` at seed 0.
fn valid(schema: &Value) -> Value {
    valid_at(schema, 0)
}

/// How many values `value` holds, itself included.
fn size(value: &Value) -> usize {
    1 + match value {
        Value::Array(items) => items.iter().map(size).sum(),
        Value::Object(fields) => fields.values().map(size).sum(),
        _ => 0,
    }
}

/// How deep `value` nests: 0 for a value that holds no other.
fn depth(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Value::Object(fields) => 1 + fields.values().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

#[test]
fn const_gives_its_value() {
    assert_eq!(
        valid(&json!({ "const": { "a": [1, 2] } })),
        json!({ "a": [1, 2] })
    );
}

#[test]
fn enum_gives_the_value_the_seed_picks() {
    let schema = json!({ "type": "string", "enum": ["celsius", "fahrenheit", "kelvin"] });
    assert_eq!(valid_at(&schema, 0), "celsius");
    assert_eq!(valid_at(&schema, 1), "fahrenheit");
    assert_eq!(valid_at(&schema, 5), "kelvin");
}

#[test]
fn examples_give_one_of_the_examples() {
    let schema = json!({ "type": "string", "examples": ["Paris", "Oslo"] });
    assert_eq!(valid_at(&schema, 0), "Paris");
    assert_eq!(valid_at(&schema, 1), "Oslo");
}

#[test]
fn all_of_is_merged_into_one_schema() {
    let schema = json!({
        "allOf": [
            { "type": "object", "properties": { "a": { "type": "integer" } }, "required": ["a"] },
            { "properties": { "b": { "type": "string" } }, "required": ["b"] }
        ]
    });
    assert_eq!(valid(&schema), json!({ "a": 0, "b": "a" }));
}

#[test]
fn all_of_follows_a_ref_among_its_members() {
    let schema = json!({
        "allOf": [{ "$ref": "#/$defs/Named" }, { "required": ["age"] }],
        "properties": { "age": { "type": "integer", "minimum": 18 } },
        "$defs": {
            "Named": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }
        }
    });
    assert_eq!(valid(&schema), json!({ "age": 18, "name": "a" }));
}

#[test]
fn a_ref_is_followed_inside_the_document() {
    let schema = json!({
        "type": "object",
        "properties": { "pet": { "$ref": "#/$defs/Pet" }, "owner": { "$ref": "#/definitions/Owner" } },
        "required": ["pet", "owner"],
        "$defs": { "Pet": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] } },
        "definitions": { "Owner": { "type": "string", "minLength": 2 } }
    });
    assert_eq!(
        valid(&schema),
        json!({ "owner": "aa", "pet": { "name": "a" } })
    );
}

#[test]
fn a_schema_that_refers_to_itself_ends_at_the_depth_limit() {
    let schema = json!({
        "$ref": "#/$defs/Node",
        "$defs": {
            "Node": { "type": "object", "properties": { "next": { "$ref": "#/$defs/Node" } }, "required": ["next"] }
        }
    });
    let value = generate(&schema, 0);
    assert!(depth(&value) <= MAX_DEPTH, "{value}");
    assert!(depth(&value) >= 3, "{value}");
}

#[test]
fn any_of_and_one_of_give_the_branch_the_seed_picks() {
    let optional = json!({ "anyOf": [{ "type": "string" }, { "type": "null" }] });
    assert_eq!(valid_at(&optional, 0), "a");
    assert_eq!(valid_at(&optional, 1), Value::Null);
    let either = json!({ "oneOf": [{ "type": "integer" }, { "type": "string" }] });
    assert_eq!(valid_at(&either, 0), 0);
    assert_eq!(valid_at(&either, 1), "b");
}

#[test]
fn a_list_of_types_gives_the_type_the_seed_picks() {
    let schema = json!({ "type": ["string", "null"] });
    assert_eq!(valid_at(&schema, 0), "a");
    assert_eq!(valid_at(&schema, 1), Value::Null);
}

#[test]
fn an_object_gets_its_required_properties_and_no_others() {
    let schema = json!({
        "type": "object",
        "properties": {
            "location": { "type": "string" },
            "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] },
            "days": { "type": "integer", "minimum": 1, "maximum": 7 },
            "notes": { "type": "string" }
        },
        "required": ["location", "unit", "days"],
        "additionalProperties": false
    });
    assert_eq!(
        valid(&schema),
        json!({ "days": 1, "location": "a", "unit": "celsius" })
    );
}

#[test]
fn an_object_takes_optional_properties_to_reach_its_least_size() {
    let schema = json!({
        "type": "object",
        "properties": { "a": { "type": "boolean" }, "b": { "type": "null" }, "c": { "type": "integer" } },
        "minProperties": 2
    });
    assert_eq!(valid(&schema), json!({ "a": false, "b": null }));
}

#[test]
fn a_map_from_any_name_gets_one_entry() {
    let schema = json!({ "type": "object", "additionalProperties": { "type": "integer" } });
    assert_eq!(valid(&schema), json!({ "additionalProperty": 0 }));
}

#[test]
fn a_required_property_without_a_schema_gets_a_value() {
    let schema = json!({ "type": "object", "required": ["anything"] });
    assert_eq!(valid(&schema), json!({ "anything": "a" }));
}

#[test]
fn an_object_is_implied_by_its_properties_and_an_array_by_its_items() {
    assert_eq!(
        valid(&json!({ "properties": { "a": {} }, "required": ["a"] })),
        json!({ "a": "a" })
    );
    assert_eq!(
        valid(&json!({ "items": { "type": "boolean" } })),
        json!([false])
    );
}

#[test]
fn a_string_gets_the_shortest_text_its_bounds_allow() {
    assert_eq!(valid(&json!({ "type": "string" })), "a");
    assert_eq!(valid(&json!({ "type": "string", "minLength": 5 })), "aaaaa");
    assert_eq!(valid(&json!({ "type": "string", "maxLength": 0 })), "");
    assert_eq!(
        valid_at(&json!({ "type": "string", "minLength": 2 }), 3),
        "dd"
    );
}

#[test]
fn a_string_with_a_known_format_gets_a_value_of_that_format() {
    for (format, value) in [
        ("date", "2024-01-01"),
        ("date-time", "2024-01-01T00:00:00Z"),
        ("time", "00:00:00Z"),
        ("duration", "P1D"),
        ("email", "user@example.com"),
        ("hostname", "example.com"),
        ("ipv4", "192.0.2.1"),
        ("ipv6", "2001:db8::1"),
        ("uri", "https://example.com/"),
        ("uuid", "00000000-0000-4000-8000-000000000000"),
    ] {
        assert_eq!(valid(&json!({ "type": "string", "format": format })), value);
    }
    assert_eq!(
        valid(&json!({ "type": "string", "format": "a-format-nobody-knows" })),
        "a"
    );
}

#[test]
fn an_integer_is_its_lowest_allowed_value_plus_the_seed_inside_the_bounds() {
    assert_eq!(valid(&json!({ "type": "integer" })), 0);
    assert_eq!(valid_at(&json!({ "type": "integer" }), 7), 7);
    assert_eq!(valid(&json!({ "type": "integer", "minimum": 5 })), 5);
    assert_eq!(valid_at(&json!({ "type": "integer", "minimum": 5 }), 2), 7);
    assert_eq!(valid(&json!({ "type": "integer", "maximum": -3 })), -3);
    assert_eq!(
        valid_at(&json!({ "type": "integer", "maximum": -3 }), 2),
        -5
    );
    assert_eq!(
        valid_at(
            &json!({ "type": "integer", "minimum": 1, "maximum": 7 }),
            12
        ),
        6
    );
    assert_eq!(
        valid(&json!({ "type": "integer", "exclusiveMinimum": 0 })),
        1
    );
    assert_eq!(
        valid(&json!({ "type": "integer", "exclusiveMaximum": 10 })),
        9
    );
    assert_eq!(
        valid(&json!({ "type": "integer", "minimum": u64::MAX })),
        u64::MAX
    );
}

#[test]
fn an_integer_moves_onto_a_multiple_of_its_step() {
    assert_eq!(
        valid(&json!({ "type": "integer", "minimum": 7, "multipleOf": 5 })),
        10
    );
    assert_eq!(
        valid_at(&json!({ "type": "integer", "multipleOf": 4 }), 9),
        8
    );
    assert_eq!(
        valid(&json!({ "type": "integer", "minimum": -7, "multipleOf": 5 })),
        -5
    );
}

#[test]
fn a_number_is_the_float_form_of_an_integer() {
    assert_eq!(valid(&json!({ "type": "number" })), 0.0);
    assert_eq!(valid(&json!({ "type": "number", "minimum": 1.5 })), 1.5);
    assert_eq!(
        valid_at(&json!({ "type": "number", "minimum": 1.5 }), 2),
        3.5
    );
    assert_eq!(valid(&json!({ "type": "number", "maximum": 2.5 })), 2.5);
    assert_eq!(
        valid(&json!({ "type": "number", "exclusiveMinimum": 0 })),
        1.0
    );
    assert_eq!(
        valid_at(&json!({ "type": "number", "minimum": 0, "maximum": 1 }), 5),
        0.0
    );
    assert_eq!(
        valid(&json!({ "type": "number", "minimum": 0.75, "multipleOf": 0.5 })),
        1.0
    );
}

#[test]
fn a_boolean_follows_the_seed_and_null_is_null() {
    assert_eq!(valid_at(&json!({ "type": "boolean" }), 0), false);
    assert_eq!(valid_at(&json!({ "type": "boolean" }), 1), true);
    assert_eq!(valid(&json!({ "type": "null" })), Value::Null);
}

#[test]
fn an_array_gets_its_shortest_allowed_list() {
    let integers = json!({ "type": "integer" });
    assert_eq!(
        valid(&json!({ "type": "array", "items": integers })),
        json!([0])
    );
    assert_eq!(
        valid(&json!({ "type": "array", "items": integers, "minItems": 3 })),
        json!([0, 0, 0])
    );
    assert_eq!(
        valid(&json!({ "type": "array", "items": integers, "maxItems": 0 })),
        json!([])
    );
    assert_eq!(valid(&json!({ "type": "array" })), json!([]));
    assert_eq!(
        valid(&json!({ "type": "array", "minItems": 2 })),
        json!(["a", "a"])
    );
}

#[test]
fn an_array_gets_one_item_for_each_prefix_item() {
    let schema = json!({
        "type": "array",
        "prefixItems": [{ "type": "string" }, { "type": "integer" }],
        "items": { "type": "boolean" },
        "minItems": 3
    });
    assert_eq!(valid(&schema), json!(["a", 0, false]));
}

#[test]
fn items_that_must_differ_do() {
    let integers = json!({ "type": "array", "items": { "type": "integer" }, "minItems": 3, "uniqueItems": true });
    assert_eq!(valid(&integers), json!([0, 1, 2]));
    let strings = json!({ "type": "array", "items": { "type": "string" }, "minItems": 3, "uniqueItems": true });
    assert_eq!(valid_at(&strings, 1), json!(["b", "c", "d"]));
}

#[test]
fn a_schema_that_allows_anything_gets_a_letter() {
    assert_eq!(valid(&json!({})), "a");
    assert_eq!(valid(&json!(true)), "a");
    assert_eq!(valid(&json!({ "description": "anything at all" })), "a");
}

#[test]
fn the_same_schema_and_seed_give_the_same_value() {
    let schema = json!({
        "type": "object",
        "properties": {
            "tags": { "type": "array", "items": { "enum": ["x", "y", "z"] }, "minItems": 2, "uniqueItems": true },
            "when": { "type": "string", "format": "date-time" },
            "score": { "type": "number", "minimum": 0, "maximum": 100 }
        },
        "required": ["tags", "when", "score"]
    });
    for seed in [0, 1, 41, u64::MAX] {
        assert_eq!(valid_at(&schema, seed), valid_at(&schema, seed));
    }
    assert_ne!(generate(&schema, 0), generate(&schema, 1));
}

#[test]
fn a_schema_that_asks_for_a_huge_value_gets_a_bounded_one() {
    let long = generate(
        &json!({ "type": "array", "items": { "type": "integer" }, "minItems": 1_000_000 }),
        0,
    );
    assert_eq!(long.as_array().map(Vec::len), Some(64));

    let wide = json!({
        "type": "array",
        "minItems": 64,
        "items": { "type": "array", "minItems": 64, "items": { "type": "integer" } }
    });
    assert!(size(&generate(&wide, 0)) <= MAX_VALUES);
    assert_eq!(
        generate(&json!({ "type": "string", "minLength": 1_000_000 }), 0)
            .as_str()
            .map(str::len),
        Some(256)
    );
}
