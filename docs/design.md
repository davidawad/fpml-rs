# Design: generating CDS bindings with xsd-parser

## Generation

The Rust package is named `fpml-rs` on Cargo and exports the `fpml` library.
`xsd-parser` 1.5.2 (MIT) can interpret the isolated 5.12 credit-derivatives
schema and emit Serde-compatible Rust with its `quick-xml` backend. The checked-in
generator defaults to the isolated CDS subset with `cargo run -p xtask -- generate`;
`cargo run -p xtask -- --full-view` selects `fpml-main-5-12.xsd` and reproduces the
interpreter panic described below. The subset output is
[`generated/fpml_cd.rs`](../generated/fpml_cd.rs).

## Full confirmation view

The complete confirmation-view entry point, `fpml-main-5-12.xsd`, does not
generate: the interpreter panics while resolving
`Type(schema=34, ns=5, name=TradeIdentifier)` referenced by
`fpml-business-events-5-12.xsd`. The checked-in distribution has all 35 confirmation XSDs, so
this is a cross-schema resolution failure, not an absent local include. The
isolated `fpml-cd-5-12.xsd` succeeds. The generator applies a custom naming
strategy to distinguish repeated schema names and renames the dynamic XSD type
alias `Option`, which otherwise shadows Rust's `Option<T>`. It also flattens
sequence wrappers that quick-xml cannot deserialize reliably. XSD enumeration
types are emitted as transparent string newtypes because quick-xml cannot read
enumerations nested inside xsd-parser's `$value` choice representation. The
four open substitution-group wrappers that xsd-parser emits as non-object-safe
Serde trait objects are retained as empty placeholders; the CDS examples do not
use them.

The isolated output maps XSD choice groups to `$value` content enums, including
CDS `generalTerms`, `feeLeg`, and `protectionTerms`. The confirmation root's
product choice/substitution-group path cannot be assessed end to end because
full-view interpretation stops at `TradeIdentifier`; there is no claim that
product substitution groups successfully round-trip. This is a clear blocker
for shipping the whole generated confirmation view without generator or schema
workarounds.

## CDS round-trip tests

The integration test processes all 24 `cd-ex*.xml` CDS examples from the schema
distribution. It parses and reserializes their XML events with `quick-xml`, then uses a typed
summary API to read the RED entity ID at
`creditDefaultSwap/generalTerms/referenceInformation/referenceEntity/entityId`,
plus the presence of `feeLeg` and `protectionTerms`. It verifies those values
remain accessible after serialization. At least 20 of the 24 examples expose a
RED identifier at that path.

The integration test also extracts each example's `creditDefaultSwap` element
and deserializes it into the generated `CreditDefaultSwapType`. This verifies
the generated CDS subset against all 24 `cd-ex*.xml` documents. It does not
deserialize the full `dataDocument` message: the isolated schema does not
generate the message-level document wrapper. String newtypes preserve each
enumeration's identity but do not validate its allowed values.

## Regenerating

`cargo run -p xtask -- generate` rewrites `generated/fpml_cd.rs` from the
checked-in 5.12 schemas. `cargo run -p xtask -- --full-view` attempts the whole
confirmation view and currently reproduces the xsd-parser panic above.
