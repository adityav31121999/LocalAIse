//! Weight quantization labels: parse them from GGUF file names and rank them.
//!
//! Q4_K_M is the default for CPU-only machines. Q5_K_M is an upgrade when RAM
//! allows, and Q8_0 is for machines with ample memory where quality matters
//! most.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quant {
    Q2K,
    Q3KM,
    Q4KM,
    Q5KM,
    Q6K,
    Q8_0,
    /// The i-quant family — smaller than Q4_K_M at the same quality.
    IQ4XS,
    /// Unquantized 16-bit weights.
    F16,
}

impl Quant {
    /// Parse the quant label from a GGUF file name, such as "Q4_K_M".
    pub fn from_filename(name: &str) -> Option<Self> {
        let upper = name.to_uppercase();
        // Check longer labels first so Q4_K_M is not matched as Q4_K.
        [
            ("IQ4_XS", Quant::IQ4XS),
            ("Q8_0", Quant::Q8_0),
            ("Q6_K", Quant::Q6K),
            ("Q5_K_M", Quant::Q5KM),
            ("Q4_K_M", Quant::Q4KM),
            ("Q3_K_M", Quant::Q3KM),
            ("Q2_K", Quant::Q2K),
            ("F16", Quant::F16),
        ]
        .into_iter()
        .find(|(label, _)| upper.contains(label))
        .map(|(_, quant)| quant)
    }

    pub fn as_label(self) -> &'static str {
        match self {
            Quant::Q2K => "Q2_K",
            Quant::Q3KM => "Q3_K_M",
            Quant::Q4KM => "Q4_K_M",
            Quant::Q5KM => "Q5_K_M",
            Quant::Q6K => "Q6_K",
            Quant::Q8_0 => "Q8_0",
            Quant::IQ4XS => "IQ4_XS",
            Quant::F16 => "F16",
        }
    }

    /// Approximate gigabytes for a model with this many parameters, so the UI
    /// can show "4.4 GB for 7B" style hints without reading the file.
    pub fn approx_size_gb(self, params_billion: f32) -> f32 {
        let bytes_per_param: f32 = match self {
            Quant::Q2K => 0.34,
            Quant::Q3KM => 0.45,
            Quant::Q4KM => 0.57,
            Quant::Q5KM => 0.70,
            Quant::Q6K => 0.85,
            Quant::Q8_0 => 1.07,
            Quant::IQ4XS => 0.53,
            Quant::F16 => 2.0,
        };
        params_billion * bytes_per_param
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_labels() {
        for (name, expected) in [
            ("example-7b-instruct.Q4_K_M.gguf", Quant::Q4KM),
            ("Llama-3.1-8B-Q8_0.gguf", Quant::Q8_0),
            ("granite-4.1-8b-instruct-Q5_K_M.gguf", Quant::Q5KM),
            ("tiny-model-Q3_K_M.gguf", Quant::Q3KM),
            ("distil-1b-Q2_K.gguf", Quant::Q2K),
        ] {
            assert_eq!(Quant::from_filename(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn filename_without_quant_is_none() {
        assert_eq!(Quant::from_filename("some-model.gguf"), None);
    }

    #[test]
    fn partially_matching_labels_do_not_confuse_longer_ones() {
        // Guard against prefix bugs by matching the full label first.
        assert_eq!(Quant::from_filename("Q4_K_M"), Some(Quant::Q4KM));
        // Q4_K_S is not in the curated set, so it stays unrecognized rather
        // than being guessed into the wrong tier.
        assert_eq!(Quant::from_filename("Q4_K_S"), None);
    }

    #[test]
    fn newer_labels_are_recognized() {
        assert_eq!(Quant::from_filename("model-Q6_K.gguf"), Some(Quant::Q6K));
        assert_eq!(Quant::from_filename("model-IQ4_XS.gguf"), Some(Quant::IQ4XS));
        assert_eq!(Quant::from_filename("model-f16.gguf"), Some(Quant::F16));
    }
}
