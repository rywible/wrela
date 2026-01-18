use strum_macros::AsRefStr;

#[derive(AsRefStr, Debug)]
pub enum Tokens {
    // class declaration
    #[strum(serialize = "A")]
    ClassA,
}
