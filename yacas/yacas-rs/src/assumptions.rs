//! Lightweight, environment-local facts about symbolic variables.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Assumption {
    Real,
    Integer,
    Positive,
    Negative,
    NonZero,
}

impl Assumption {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "Real" => Some(Self::Real),
            "Integer" => Some(Self::Integer),
            "Positive" => Some(Self::Positive),
            "Negative" => Some(Self::Negative),
            "NonZero" => Some(Self::NonZero),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Real => "Real",
            Self::Integer => "Integer",
            Self::Positive => "Positive",
            Self::Negative => "Negative",
            Self::NonZero => "NonZero",
        }
    }

    fn sort_key(self) -> u8 {
        match self {
            Self::Real => 0,
            Self::Integer => 1,
            Self::Positive => 2,
            Self::Negative => 3,
            Self::NonZero => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssumptionError {
    pub symbol: String,
    pub fact: Assumption,
    pub conflicts_with: Assumption,
}

#[derive(Debug, Clone, Default)]
pub struct AssumptionContext {
    facts: HashMap<String, HashSet<Assumption>>,
}

impl AssumptionContext {
    pub fn assume(&mut self, symbol: &str, fact: Assumption) -> Result<(), AssumptionError> {
        let conflict = match fact {
            Assumption::Positive => Some(Assumption::Negative),
            Assumption::Negative => Some(Assumption::Positive),
            _ => None,
        };
        if let Some(conflicts_with) = conflict {
            if self.is_assumed(symbol, conflicts_with) {
                return Err(AssumptionError {
                    symbol: symbol.into(),
                    fact,
                    conflicts_with,
                });
            }
        }

        let facts = self.facts.entry(symbol.into()).or_default();
        facts.insert(fact);
        match fact {
            Assumption::Integer => {
                facts.insert(Assumption::Real);
            }
            Assumption::Positive | Assumption::Negative => {
                facts.insert(Assumption::Real);
                facts.insert(Assumption::NonZero);
            }
            Assumption::Real | Assumption::NonZero => {}
        }
        Ok(())
    }

    pub fn is_assumed(&self, symbol: &str, fact: Assumption) -> bool {
        self.facts
            .get(symbol)
            .is_some_and(|facts| facts.contains(&fact))
    }

    pub fn clear(&mut self) {
        self.facts.clear();
    }

    pub fn all(&self) -> Vec<(String, Assumption)> {
        let mut facts: Vec<_> = self
            .facts
            .iter()
            .flat_map(|(symbol, facts)| facts.iter().map(|fact| (symbol.clone(), *fact)))
            .collect();
        facts.sort_by(|(left_symbol, left_fact), (right_symbol, right_fact)| {
            left_symbol
                .cmp(right_symbol)
                .then_with(|| left_fact.sort_key().cmp(&right_fact.sort_key()))
        });
        facts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Environment;
    use crate::evaluator::eval;
    use crate::parser::parse_expression;
    use crate::printer::infix_print;

    fn run(env: &mut Environment, source: &str) -> String {
        let expression = parse_expression(env, &format!("{source};"))
            .unwrap()
            .expect("expression");
        let result = eval(env, &expression).unwrap();
        infix_print(env, &result)
    }

    #[test]
    fn derives_safe_facts_and_rejects_conflicting_signs() {
        let mut context = AssumptionContext::default();
        context.assume("n", Assumption::Integer).unwrap();
        context.assume("n", Assumption::Positive).unwrap();
        assert!(context.is_assumed("n", Assumption::Integer));
        assert!(context.is_assumed("n", Assumption::Real));
        assert!(context.is_assumed("n", Assumption::NonZero));
        assert!(context.assume("n", Assumption::Negative).is_err());
        assert!(!context.is_assumed("n", Assumption::Negative));
    }

    #[test]
    fn language_commands_are_environment_local() {
        let mut first = Environment::new();
        let mut second = Environment::new();
        assert_eq!(run(&mut first, "ListAssumptions()"), "{}");
        assert_eq!(run(&mut first, "Assume(n,Integer)"), "True");
        assert_eq!(run(&mut first, "Assume(n,Positive)"), "True");
        assert_eq!(run(&mut first, "Assume(x,Negative)"), "True");
        assert_eq!(
            run(&mut first, "ListAssumptions()"),
            "{{n,Real},{n,Integer},{n,Positive},{n,NonZero},{x,Real},{x,Negative},{x,NonZero}}"
        );
        assert_eq!(run(&mut first, "IsAssumed(n,Real)"), "True");
        assert_eq!(run(&mut first, "Set(n,2)"), "2");
        assert_eq!(run(&mut first, "IsAssumed(n,Positive)"), "True");
        assert_eq!(run(&mut second, "IsAssumed(n,Integer)"), "False");
        assert_eq!(run(&mut first, "ClearAssumptions()"), "True");
        assert_eq!(run(&mut first, "ListAssumptions()"), "{}");
        assert_eq!(run(&mut first, "IsAssumed(n,Integer)"), "False");
    }

    #[test]
    fn nested_scopes_restore_previous_facts() {
        let mut env = Environment::new();
        assert_eq!(run(&mut env, "Assume(x,Real)"), "True");
        assert_eq!(run(&mut env, "PushAssumptions()"), "True");
        assert_eq!(run(&mut env, "Assume(x,Positive)"), "True");
        assert_eq!(run(&mut env, "PushAssumptions()"), "True");
        assert_eq!(run(&mut env, "ClearAssumptions()"), "True");
        assert_eq!(run(&mut env, "IsAssumed(x,Real)"), "False");
        assert_eq!(run(&mut env, "PopAssumptions()"), "True");
        assert_eq!(run(&mut env, "IsAssumed(x,Positive)"), "True");
        assert_eq!(run(&mut env, "PopAssumptions()"), "True");
        assert_eq!(run(&mut env, "IsAssumed(x,Real)"), "True");
        assert_eq!(run(&mut env, "IsAssumed(x,Positive)"), "False");
        assert!(!env.pop_assumptions());
    }
}
