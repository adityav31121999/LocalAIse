#[derive(Clone, Debug)]
pub struct Fact {
    pub sub_q: usize,
    pub source_id: i64,
    pub claim: String,
    pub quote: String,
}