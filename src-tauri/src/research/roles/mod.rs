pub mod reader;
pub mod gap;
pub mod writer;

pub fn plan_subquestions(question: &str) -> Vec<String> {
    vec![question.to_string()]
}