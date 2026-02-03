# Builder Invites (Grantor + TTL Policy)

Builder invites are time-bounded grants that enable **write access** (e.g. git push / privileged UI actions) for a specific repo or domain-scoped workspace.

## Grantor policy

An invite MAY be granted by any of:

- **BDFL** (global override)
- **Stewards** (global governance role)
- **Domain maintainers** for the target repo/domain (scoped)

Grantors MUST record `granted_by` and their role/capability in the audit log.

## Default TTL

- **Default TTL**: 14 days
- Invites MUST have an explicit `expires_at`.
- The system MUST treat invites as inactive once `now >= expires_at`.

## Renewal

- Any valid grantor (per policy above) MAY renew an active invite.
- Renewal sets `expires_at = now + 14 days` (i.e. renewal extends from the time of renewal, not from the prior expiry).
- Renewals MUST be audited as a distinct event.

### Limits

- A single renewal MAY NOT set `expires_at` more than **90 days** into the future.
- Repeated renewals are allowed, but SHOULD be rare and justified; stewards MAY revoke invites with poor justification.

## Revocation

- Any valid grantor MAY revoke an invite at any time (immediate effect).
- Revocations MUST be audited.

## Implementation notes (for Phase 1/6)

The authz layer MUST check builder invite validity as:

- `revoked_at` is null
- `now < expires_at`
- invite scope matches the target repo/domain

