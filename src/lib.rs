//! Source-preserving selection over caller-supplied evidence.
pub mod adapters;
pub mod query_expansion;
pub mod selection;
pub mod verified_query_rules;

#[doc(hidden)]
pub mod budget;
#[doc(hidden)]
pub mod normalize;
#[doc(hidden)]
pub mod rank;
