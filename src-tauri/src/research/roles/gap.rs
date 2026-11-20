use crate::research::fact::Fact;

pub fn check(sub_count: usize, facts: &[Fact]) -> Vec<usize> {
    (0..sub_count).filter(|i| !facts.iter().any(|f| f.sub_q == *i)).collect()
}