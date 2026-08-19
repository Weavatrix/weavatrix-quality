//! Versioned metamorphic relations. Proposed ones cannot run until sealed.

use thiserror::Error;

/// Why a relation could not execute.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MetaError {
    /// Agent-proposed relation is not sealed.
    #[error("unsealed metamorphic relation cannot execute")]
    Unsealed,
    /// Schema is not `1`.
    #[error("unknown metamorphic schema_v {0}")]
    UnknownSchema(u32),
}

/// How the relation was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationOrigin {
    /// Built-in numeric/collection/aggregation relation.
    Builtin,
    /// Agent-proposed. Requires review + seal.
    Proposed,
}

/// Input transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaTransform {
    /// Permute values.
    Permute,
    /// Append a zero record.
    AppendZero,
    /// Scale by 2.
    Scale2,
    /// Split then recombine.
    SplitRecombine,
    /// Viewport change; semantics must hold.
    Viewport,
}

/// Expected relationship after transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaExpectation {
    /// Aggregate unchanged.
    AggregateUnchanged,
    /// SUM unchanged.
    SumUnchanged,
    /// SUM doubles.
    SumDoubles,
    /// Total conserved.
    TotalConserved,
    /// Data semantics unchanged.
    SemanticsUnchanged,
}

/// Versioned relation. Subsequent executions are model-less.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetamorphicRelation {
    /// Schema. Only `1`.
    pub schema_v: u32,
    /// Relation id.
    pub id: String,
    /// Origin.
    pub origin: RelationOrigin,
    /// QA seal. Built-ins start sealed.
    pub sealed: bool,
    /// Transform.
    pub transform: MetaTransform,
    /// Expectation.
    pub expectation: MetaExpectation,
}

/// Numeric/collection sample plus optional semantic digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaSample {
    /// Ordered values.
    pub values: Vec<i64>,
    /// Semantic digest (viewport / data class).
    pub semantic: Option<String>,
}

/// Built-in sealed relations. No AI.
#[must_use]
pub fn builtins() -> Vec<MetamorphicRelation> {
    vec![
        builtin(
            "permute-aggregate",
            MetaTransform::Permute,
            MetaExpectation::AggregateUnchanged,
        ),
        builtin(
            "append-zero-sum",
            MetaTransform::AppendZero,
            MetaExpectation::SumUnchanged,
        ),
        builtin(
            "scale2-sum",
            MetaTransform::Scale2,
            MetaExpectation::SumDoubles,
        ),
        builtin(
            "split-recombine-total",
            MetaTransform::SplitRecombine,
            MetaExpectation::TotalConserved,
        ),
        builtin(
            "viewport-semantics",
            MetaTransform::Viewport,
            MetaExpectation::SemanticsUnchanged,
        ),
    ]
}

fn builtin(
    id: &str,
    transform: MetaTransform,
    expectation: MetaExpectation,
) -> MetamorphicRelation {
    MetamorphicRelation {
        schema_v: 1,
        id: id.to_owned(),
        origin: RelationOrigin::Builtin,
        sealed: true,
        transform,
        expectation,
    }
}

/// Agent proposal. Cannot execute until [`seal_relation`].
#[must_use]
pub fn propose(
    id: impl Into<String>,
    transform: MetaTransform,
    expectation: MetaExpectation,
) -> MetamorphicRelation {
    MetamorphicRelation {
        schema_v: 1,
        id: id.into(),
        origin: RelationOrigin::Proposed,
        sealed: false,
        transform,
        expectation,
    }
}

/// QA review seal. Later runs stay model-less.
#[must_use]
pub fn seal_relation(mut relation: MetamorphicRelation) -> MetamorphicRelation {
    relation.sealed = true;
    relation
}

/// Execute a sealed relation. No LLM.
///
/// # Errors
///
/// Unsealed or unknown schema.
pub fn execute(relation: &MetamorphicRelation, sample: &MetaSample) -> Result<bool, MetaError> {
    if relation.schema_v != 1 {
        return Err(MetaError::UnknownSchema(relation.schema_v));
    }
    if !relation.sealed {
        return Err(MetaError::Unsealed);
    }
    let before = sample.values.iter().copied().sum::<i64>();
    let after_values = transform_values(relation.transform, &sample.values);
    let after = after_values.iter().copied().sum::<i64>();
    let holds = match relation.expectation {
        MetaExpectation::AggregateUnchanged
        | MetaExpectation::SumUnchanged
        | MetaExpectation::TotalConserved => before == after,
        MetaExpectation::SumDoubles => after == before.saturating_mul(2),
        MetaExpectation::SemanticsUnchanged => sample.semantic.is_some(),
    };
    Ok(holds)
}

fn transform_values(transform: MetaTransform, values: &[i64]) -> Vec<i64> {
    match transform {
        MetaTransform::Permute => {
            let mut out = values.to_vec();
            out.reverse();
            out
        }
        MetaTransform::AppendZero => {
            let mut out = values.to_vec();
            out.push(0);
            out
        }
        MetaTransform::Scale2 => values.iter().map(|item| item.saturating_mul(2)).collect(),
        MetaTransform::SplitRecombine => {
            let mid = values.len() / 2;
            let mut out = values[mid..].to_vec();
            out.extend_from_slice(&values[..mid]);
            out
        }
        MetaTransform::Viewport => values.to_vec(),
    }
}
