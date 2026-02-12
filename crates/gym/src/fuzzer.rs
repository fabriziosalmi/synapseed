//! Fuzz test generator — auto-generates proptest property tests from source code.
//!
//! Scans `pub fn` signatures with regex, maps parameter types to proptest
//! strategies, and emits a complete `tests/fuzz.rs` file.

use regex::Regex;

/// A parsed function signature eligible for fuzz testing.
struct FuzzTarget {
    name: String,
    params: Vec<(String, String)>, // (param_name, type)
}

/// Generate proptest fuzz tests from source code.
///
/// Returns `None` if no public functions with fuzzable parameter types are found.
pub fn generate_fuzz_tests(source: &str) -> Option<String> {
    let targets = extract_fuzz_targets(source);
    if targets.is_empty() {
        return None;
    }

    let mut output = String::from("use proptest::prelude::*;\nuse eval_project::*;\n\n");
    output.push_str("proptest! {\n");

    for target in &targets {
        let param_strats: Vec<String> = target
            .params
            .iter()
            .map(|(name, ty)| format!("{name} in {}", type_to_strategy(ty)))
            .collect();

        output.push_str(&format!(
            "    #[test]\n    fn fuzz_{}({}) {{\n        let _ = {}({});\n    }}\n\n",
            target.name,
            param_strats.join(", "),
            target.name,
            target.params.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", "),
        ));
    }

    output.push_str("}\n");
    Some(output)
}

/// Extract public function signatures that have fuzzable parameters.
fn extract_fuzz_targets(source: &str) -> Vec<FuzzTarget> {
    let fn_re = Regex::new(
        r"pub\s+fn\s+(\w+)\s*\(([^)]*)\)\s*(?:->\s*[^{]+)?\s*\{"
    ).expect("valid regex");

    let mut targets = Vec::new();

    for cap in fn_re.captures_iter(source) {
        let name = cap[1].to_string();
        let params_str = cap[2].trim();

        if params_str.is_empty() {
            continue;
        }

        // Skip methods with self parameter.
        if params_str.contains("self") {
            continue;
        }

        let mut params = Vec::new();
        let mut all_fuzzable = true;

        for param in split_params(params_str) {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }

            // Parse "name: Type"
            if let Some((pname, ptype)) = param.split_once(':') {
                let pname = pname.trim().to_string();
                let ptype = ptype.trim().to_string();

                if is_fuzzable_type(&ptype) {
                    params.push((pname, ptype));
                } else {
                    all_fuzzable = false;
                    break;
                }
            }
        }

        if all_fuzzable && !params.is_empty() {
            targets.push(FuzzTarget { name, params });
        }
    }

    targets
}

/// Split parameter list respecting nested angle brackets (for generics).
fn split_params(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in s.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

/// Check if a type can be mapped to a proptest strategy.
fn is_fuzzable_type(ty: &str) -> bool {
    type_to_strategy_opt(ty).is_some()
}

/// Map a Rust type to a proptest strategy string.
fn type_to_strategy(ty: &str) -> String {
    type_to_strategy_opt(ty).unwrap_or_else(|| format!("any::<{ty}>()"))
}

/// Try to map a Rust type to a proptest strategy. Returns None for unfuzzable types.
fn type_to_strategy_opt(ty: &str) -> Option<String> {
    let ty = ty.trim();

    // References and slices — skip (lifetime issues in proptest).
    if ty.starts_with('&') {
        return None;
    }

    // Primitive integers and floats.
    match ty {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
        | "u128" | "usize" | "f32" | "f64" | "bool" | "char" => {
            return Some(format!("any::<{ty}>()"));
        }
        "String" => {
            return Some("any::<String>()".to_string());
        }
        _ => {}
    }

    // Vec<T>
    if let Some(inner) = strip_generic(ty, "Vec") {
        let inner_strat = type_to_strategy_opt(inner)?;
        return Some(format!("proptest::collection::vec({inner_strat}, 0..10)"));
    }

    // Option<T>
    if let Some(inner) = strip_generic(ty, "Option") {
        let inner_strat = type_to_strategy_opt(inner)?;
        return Some(format!("proptest::option::of({inner_strat})"));
    }

    None
}

/// Extract the inner type from `Wrapper<Inner>`.
fn strip_generic<'a>(ty: &'a str, wrapper: &str) -> Option<&'a str> {
    let ty = ty.trim();
    if ty.starts_with(wrapper) {
        let rest = ty[wrapper.len()..].trim();
        if rest.starts_with('<') && rest.ends_with('>') {
            return Some(&rest[1..rest.len() - 1]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_simple_function() {
        let source = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let result = generate_fuzz_tests(source).unwrap();
        assert!(result.contains("fn fuzz_add"));
        assert!(result.contains("any::<i32>()"));
        assert!(result.contains("let _ = add(a, b)"));
    }

    #[test]
    fn test_generate_string_param() {
        let source = r#"
pub fn greet(name: String) -> String {
    format!("Hello, {name}")
}
"#;
        let result = generate_fuzz_tests(source).unwrap();
        assert!(result.contains("fn fuzz_greet"));
        assert!(result.contains("any::<String>()"));
    }

    #[test]
    fn test_skip_reference_params() {
        let source = r#"
pub fn len(s: &str) -> usize {
    s.len()
}
"#;
        let result = generate_fuzz_tests(source);
        assert!(result.is_none());
    }

    #[test]
    fn test_skip_self_methods() {
        let source = r#"
pub fn process(&self, x: i32) -> i32 {
    x + 1
}
"#;
        let result = generate_fuzz_tests(source);
        assert!(result.is_none());
    }

    #[test]
    fn test_skip_no_pub_functions() {
        let source = r#"
fn private(a: i32) -> i32 {
    a * 2
}
"#;
        let result = generate_fuzz_tests(source);
        assert!(result.is_none());
    }

    #[test]
    fn test_vec_and_option_params() {
        let source = r#"
pub fn sum_all(values: Vec<i32>, offset: Option<i32>) -> i32 {
    values.iter().sum::<i32>() + offset.unwrap_or(0)
}
"#;
        let result = generate_fuzz_tests(source).unwrap();
        assert!(result.contains("fn fuzz_sum_all"));
        assert!(result.contains("proptest::collection::vec"));
        assert!(result.contains("proptest::option::of"));
    }

    #[test]
    fn test_multiple_functions() {
        let source = r#"
pub fn add(a: i32, b: i32) -> i32 { a + b }
pub fn mul(a: f64, b: f64) -> f64 { a * b }
"#;
        let result = generate_fuzz_tests(source).unwrap();
        assert!(result.contains("fn fuzz_add"));
        assert!(result.contains("fn fuzz_mul"));
    }
}
