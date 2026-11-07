use std::collections::HashMap;
use crate::research::fact::Fact;
use crate::research::roles::reader::norm;
use crate::research::store::EvidenceStore;

pub struct Verdict { pub supported: Vec<Fact>, pub unsupported: Vec<Fact> }

pub fn verify(facts: Vec<Fact>, store: &EvidenceStore) -> Verdict {
    let normed: HashMap<i64, String> =
        store.sources().iter().map(|s| (s.id, norm(&s.text))).collect();
    let (mut supported, mut unsupported) = (Vec::new(), Vec::new());
    for f in facts {
        let q = norm(&f.quote);
        let ok = !q.is_empty() && normed.get(&f.source_id).map_or(false, |t| t.contains(&q));
        if ok { supported.push(f) } else { unsupported.push(f) }
    }
    Verdict { supported, unsupported }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fact(source_id: i64, quote: &str) -> Fact {
        Fact { sub_q: 0, source_id, claim: quote.into(), quote: quote.into() }
    }

    #[test]
    fn splits_supported_from_fabricated() {
        let mut store = EvidenceStore::new();
        let id = store.add_source("file:a.md", "a", "Alpha  beta\ngamma delta epsilon zeta.");
        let v = verify(vec![
            fact(id, "alpha beta gamma delta"),
            fact(id, "this sentence was never in the source"),
            fact(999, "alpha beta gamma delta"),
        ], &store);
        assert_eq!(v.supported.len(), 1);
        assert_eq!(v.unsupported.len(), 2);
    }
}