# Event Contracts

This directory is the declared path for event-contract documentation and
versioned event payload definitions.

The contract is append-only in practice:

- add fields before removing or renaming existing ones
- keep consumers tolerant of unknown fields
- preserve stable event names and identifiers
- document schema or wire changes here before broad rollout

The behavior behind the contract is implemented in the Rust event and state
surfaces, primarily:

- `src/git/event.rs`
- `src/engine.rs`
- `src/state.rs`

If this directory is being checked by an audit, its presence means the event
contract path exists even when the concrete schema files live elsewhere.
