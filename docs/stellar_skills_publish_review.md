# Stellar Skills Publish Review

This review prepares the NeuroChain Stellar Guardrails skill for the Stellar
Skills community directory without publishing it.

Status:

```text
distribution_channel = skills.stellar.org community skills
publish_candidate = true
published = false
external_pull_request_created = false
runtime_dependency = false
submit_surface = false
```

## Official Directory Contract

The Stellar Skills community directory currently asks contributors to open a
pull request that adds one entry to `ECOSYSTEM_CARDS` in
`site/src/data/skills.ts`:

```text
https://skills.stellar.org/
```

The entry contains:

- `title`
- a verb-led `description`
- `pathLabel`
- `copyValue`, pointing directly to the public `SKILL.md`

The prepared entry lives at:

```text
distribution/stellar-skills-community-card.json
```

It is review evidence only. Adding the entry to an external repository and
opening a pull request require a separate explicit publication decision.

## Candidate Review

| Check | Result |
| --- | --- |
| Title names the product clearly | Pass |
| Description begins with an action verb | Pass |
| Description covers Stellar ActionPlans and deterministic guardrails | Pass |
| Description distinguishes read-only verification from submit permission | Pass |
| Repository label points to the public project | Pass |
| Direct link targets the canonical `SKILL.md` on `main` | Pass after the skill branch is merged |
| Skill frontmatter contains only `name` and `description` | Pass |
| Skill remains an instruction/distribution layer | Pass |
| Runtime dependency added | No |
| Submit surface added | No |
| External publication performed | No |

## Publication Preconditions

Before creating the external directory pull request:

1. merge the skill package into the public repository's `main` branch
2. confirm the `copyValue` URL returns the canonical `SKILL.md`
3. run the MCP and skill release-candidate gate
4. run an external MCP host or Inspector validation when an approved host is
   available
5. review the directory card for truthful, bounded product claims
6. obtain explicit approval to create the external pull request

The directory card must not advertise signing, broadcasting, testnet
attestation submit, nullifier consume, or underlying ActionPlan execution.
