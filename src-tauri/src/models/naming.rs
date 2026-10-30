//! Naming helpers: readable names for files and folders, and the parameter
//! count a model file name states.

/// A readable name from a file or folder name.
///
/// "granite-4.2-3b-q4_k_m.gguf" becomes "granite 4.2 3b q4 k m".
pub fn label(stem: &str) -> String {
    stem.replace(['-', '_'], " ").trim().to_string()
}

/// A comparable form of a name: lowercase and without separators.
///
/// Used to decide whether a file belongs to a folder ("granite-4.2-3b" and
/// "granite-4.2-3b-q4_k_m.gguf" both normalize to the same prefix).
pub fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// How many leading characters two names share.
pub fn common_prefix_len(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count()
}

/// The parameter count in billions, when the name states one.
///
/// Model files are named after their size — "qwen2.5-7b-instruct",
/// "granite-4.2-3b" — so the number in front of the "b" is the parameter
/// count in billions.
pub fn params_billion_from_name(name: &str) -> Option<f32> {
    let mut best: Option<f32> = None;

    for token in name.split(|c: char| !c.is_ascii_alphanumeric() && c != '.') {
        let stem = match token.strip_suffix('b').or_else(|| token.strip_suffix('B')) {
            Some(stem) => stem,
            None => continue,
        };
        if stem.is_empty() || !stem.contains(|c: char| c.is_ascii_digit()) {
            continue;
        }

        // Only digits and a single decimal point are plausible.
        let mut dots = 0;
        let plausible = stem.chars().all(|c| {
            if c == '.' {
                dots += 1;
                dots <= 1
            } else {
                c.is_ascii_digit()
            }
        });
        if !plausible {
            continue;
        }

        let Ok(value) = stem.parse::<f32>() else {
            continue;
        };
        if value <= 0.0 || value > 1000.0 {
            continue;
        }

        best = Some(match best {
            Some(current) if current >= value => current,
            _ => value,
        });
    }

    best
}

/// Returns true if the model name/architecture suggests it supports reasoning/thinking.
/// These models natively produce reasoning traces (chain-of-thought).
pub fn is_reasoning_model(name: &str, architecture: &str) -> bool {
    let normalized_name = normalize(name);
    let normalized_arch = normalize(architecture);

    // Known reasoning model families (by architecture prefix or name patterns)
    const REASONING_ARCHITECTURES: &[&str] = &[
        "qwen3",      // Qwen3 supports thinking mode
        "deepseek",   // DeepSeek R1, etc.
        "qwq",        // QwQ
        "nemotron",   // Nemotron 3 Ultra
        "marco",      // Marco-o1
        "skywork",    // Skywork-o1
    ];

    const REASONING_NAME_PATTERNS: &[&str] = &[
        "r1",
        "reasoning",
        "thinking",
        "cot",
        "o1",
        "qwq",
        "qwen3",
        "deepseekr1",
        "nemotron3ultra",
        "marcoo1",
    ];

    // Check architecture
    for arch in REASONING_ARCHITECTURES {
        if normalized_arch.contains(arch) {
            return true;
        }
    }

    // Check name patterns
    for pattern in REASONING_NAME_PATTERNS {
        if normalized_name.contains(pattern) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_parameter_count_from_a_model_name() {
        assert_eq!(params_billion_from_name("qwen2.5-7b-instruct-q4_k_m"), Some(7.0));
        assert_eq!(params_billion_from_name("granite-4.2-3b"), Some(3.0));
        assert_eq!(params_billion_from_name("llama-3.1-8B-instruct"), Some(8.0));
        assert_eq!(params_billion_from_name("gemma-3-0.6b-it"), Some(0.6));
    }

    #[test]
    fn a_name_without_a_size_gives_no_count() {
        assert_eq!(params_billion_from_name("Phi-3.5-mini-instruct"), None);
        assert_eq!(params_billion_from_name("mmproj-model-f16"), None);
        assert_eq!(params_billion_from_name("model-4bit"), None);
    }

    #[test]
    fn a_quant_label_is_not_a_parameter_count() {
        assert_eq!(params_billion_from_name("model-q4_k_m.gguf"), None);
        assert_eq!(params_billion_from_name("model-Q8_0.gguf"), None);
    }

    #[test]
    fn normalization_drops_separators() {
        assert_eq!(normalize("granite-4.2-3b"), "granite423b");
        assert_eq!(normalize("Granite_4_2_3B"), "granite423b");
    }

    #[test]
    fn detects_reasoning_models_by_architecture() {
        assert!(is_reasoning_model("qwen3-8b", "qwen3"));
        assert!(is_reasoning_model("deepseek-r1", "deepseek"));
        assert!(is_reasoning_model("qwq-32b", "qwq"));
        assert!(is_reasoning_model("nemotron-3-ultra", "nemotron"));
    }

    #[test]
    fn detects_reasoning_models_by_name_patterns() {
        assert!(is_reasoning_model("my-model-r1", "llama"));
        assert!(is_reasoning_model("qwen3-custom", "llama"));
        assert!(is_reasoning_model("deepseek-r1-distill", "llama"));
        assert!(is_reasoning_model("qwq-test", "llama"));
    }

    #[test]
    fn non_reasoning_models_return_false() {
        assert!(!is_reasoning_model("llama-3.1-8b", "llama"));
        assert!(!is_reasoning_model("qwen2.5-7b", "qwen2"));
        assert!(!is_reasoning_model("gemma-7b", "gemma"));
        assert!(!is_reasoning_model("granite-4.2-3b", "granite"));
        assert!(!is_reasoning_model("phi-3.5", "phi"));
        assert!(!is_reasoning_model("mistral-7b", "mistral"));
    }
}
