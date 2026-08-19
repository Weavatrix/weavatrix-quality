# Delta for Sankey

## ADDED Requirements

### Requirement: Visual Limit Others
Overflow values SHALL be represented by an Others node when cardinality exceeds the configured visual limit.

#### Scenario: Overflow grouped
- GIVEN a Sankey chart with cardinality above the visual limit
- WHEN the chart is rendered
- THEN an Others node is visible
- AND overflow values are grouped into that node

## MODIFIED Requirements

### Requirement: Visual Limit
The system SHALL group overflow values instead of rendering every series past the visual limit.

#### Scenario: Exact limit
- GIVEN cardinality equal to the visual limit
- WHEN the chart is rendered
- THEN no Others node is shown
