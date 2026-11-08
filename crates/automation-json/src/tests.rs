use super::*;
#[test]
fn node_ids_keep_native_spellings_and_large_integer_precision() {
    for (input, expected) in [(json!(12), 12), (json!(" 0:12 "), 12), (json!("prefix:42"), 42), (json!(u64::MAX), u64::MAX)] {
        assert_eq!(id_of(&input), Some(expected));
    }
    for input in [json!("+12"), json!("0:+12"), json!("-1"), json!(true), json!(12.5), json!("18446744073709551616"), Value::Null] {
        assert_eq!(id_of(&input), None);
    }
}
#[test]
fn numeric_coercion_stays_finite_and_preserves_existing_units() {
    for (input, expected) in [(json!(" 12px "), 12.0), (json!("50%"), 50.0), (json!(true), 1.0), (json!(false), 0.0), (json!(-2), -2.0)] {
        assert_eq!(f64_of(&input), Some(expected));
    }
    for input in [json!("NaN"), json!("inf"), json!("1e999"), json!([]), Value::Null] {
        assert_eq!(f64_of(&input), None);
    }
}
#[test]
fn paint_normalization_preserves_css_gradient_axis_and_depth_guard() {
    let mut gradient = json!({"type":"linearGradient","from":[0.2,0.3],"to":[0.8,0.9],"stops":["#000000","#ffffff"]});
    normalize_paint(&mut gradient);
    assert_eq!(gradient["type"], "gradient");
    assert_eq!(gradient["kind"], "linear");
    let width = gradient["width"].as_array().unwrap();
    assert!((width[0].as_f64().unwrap() + 0.4).abs() < 1e-12);
    assert!((width[1].as_f64().unwrap() - 0.9).abs() < 1e-12);
    assert_eq!(gradient["stops"][0]["position"], 0.0);
    assert_eq!(gradient["stops"][1]["color"], json!({"r":1.0,"g":1.0,"b":1.0,"a":1.0}));
    let mut nested = json!({"color":"#ff0000"});
    for _ in 0..20 {
        nested = json!([nested]);
    }
    normalize_paint(&mut nested);
    let mut item = &nested;
    for _ in 0..20 {
        item = &item[0];
    }
    assert_eq!(item["color"], "#ff0000", "normalization stops before excessive nesting");
}
#[test]
fn inspection_options_keep_shared_validation_and_structural_fields() {
    let outline = InspectOptions::parse(&json!({"outline":true})).unwrap();
    assert!(outline.outline());
    assert!(outline.includes("name"));
    assert!(!outline.includes("fontSize"));
    let empty = InspectOptions::parse(&json!({"fields":[]})).unwrap();
    assert!(!empty.includes("name"));
    assert!(!empty.explicit("visible"));
    assert!(InspectOptions::parse(&json!({"fields":["visible"]})).unwrap().explicit("visible"));
    for input in [json!({"outline":true,"fields":[]}), json!({"outline":null}), json!({"fields":[1]}), json!({"fields":["not-a-field"]})] {
        assert!(InspectOptions::parse(&input).is_err());
    }
}
