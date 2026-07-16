# Publishing the PDF crate family to crates.io

This is the authoritative procedure for releasing the PDF crate family to
crates.io.  pdf-maker and pdf-dump link back here; this file owns the full
picture.

The model is **publish-only**: your `/commit-*` skills (via `verset`) already
own version numbers and bump + commit + push on every change, so crates.io
simply lags behind the working tree.  Publishing does **not** bump anything — it
uploads the already-committed current version and tags it.  We therefore use
cargo-release’s individual **step** subcommands (`publish`, `tag`, `push`) and
never `cargo release <level>` / `cargo release version`, which would bump.

## The family, and the order

```
medpdf          (workspace root lib — depends on nothing internal)
  └─ medpdf-image   (depends on medpdf)
       └─ pdf-maker (depends on medpdf + medpdf-image)   ← separate repo

pdf-dump        (independent — no medpdf deps; publish anytime)
```

A published crate cannot carry `path =` dependencies, so the chain must go
**bottom-up**: a crate can only publish once every crate it depends on is
already on crates.io at the pinned version.  Order:

1. **medpdf** then **medpdf-image** — one `--workspace` release in this repo.
2. **pdf-maker** — in its own repo, only after step 1 is live on crates.io.
3. **pdf-dump** — independent; any time, in its own repo.

`pdf-test-visual` (the third workspace member) is `publish = false` and carries
`[package.metadata.release] release = false`, so cargo-release skips it.

## When to publish — `scripts/publish-status.py`

Publish **releases, not commits.**  crates.io versions are permanent and
immutable (you can yank, never edit or delete), so you publish when a _consumer_
of the crate would care about what has accumulated — a feature, an API change,
or a bug fix that affects them — not on every commit.

To see what has drifted at a glance:

```
python3 scripts/publish-status.py          # human table
python3 scripts/publish-status.py --json    # machine-readable
```

For each crate it shows the local version, the crates.io version, the number of
commits touching `src/` since the published version, and a verdict.  Exit codes
(portfolio-canonical): `0` all up-to-date · `3` findings — something is worth
publishing · `1` tool error / crates.io unreachable (status could not be
confirmed clean).  A `~` on the commit count means it was derived from the
manifest because no release tag exists yet; the count becomes exact once the
first tagged publish lands.

## How to publish

Preconditions (all of them): you are on `main`, the working tree is clean, and
everything is pushed.  `verset` has already set each crate to the version you
intend to publish.  **Every command below is dry-run by default** — it prints
what it would do and uploads nothing.  Add `-x` (`--execute`) to perform it.

### Step 1 — medpdf + medpdf-image (this repo)

```
cargo release publish --workspace              # dry-run preview
cargo release publish --workspace -x           # publish medpdf, then medpdf-image
cargo release tag --workspace -x               # tag medpdf-v<ver>, medpdf-image-v<ver>
cargo release push --workspace -x              # push the tags
```

cargo-release publishes workspace members in dependency order and waits for the
crates.io index to catch up between them.  (In a _dry-run alone_, medpdf-image’s
verify step can fail because it wants medpdf’s new version from crates.io, which
a dry-run has not uploaded — that is expected and resolves under `-x`, where
medpdf is published first.)

### Step 2 — pdf-maker (its own repo)

Only after step 1 is live on crates.io.  pdf-maker pins `medpdf` and
`medpdf-image`; if you are adopting new versions of them, that pin bump is a
normal code change committed via `/commit-rust-cli` first (as was done for
medpdf 0.11.0).  Then, in the pdf-maker repo:

```
cargo release publish        # dry-run
cargo release publish -x     # upload current version
cargo release tag -x         # tag pdf-maker-v<ver>
cargo release push -x        # push the tag
```

If pdf-maker’s publish fails to resolve medpdf from crates.io, the index has not
propagated yet — wait a minute and retry.

### Step 3 — pdf-dump (its own repo, independent)

```
cargo release publish        # dry-run
cargo release publish -x
cargo release tag -x
cargo release push -x
```

## Why publish-only, and not `cargo release <level>`

`cargo release <level>` (or `cargo release version`) bumps the version, writes a
`chore: Release …` commit, tags, publishes, and pushes — all in one shot.  That
duplicates and fights the `verset` + `/commit-*` workflow, which already bumps
and commits on every change and also runs the fmt gate and `install-bin`.  So
here cargo-release is used **only** for the steps that workflow does _not_ do:
`publish`, `tag`, `push`.  Versioning stays with the commit skills.

Per-repo `release.toml` files pin two things: `allow-branch = ["main"]` and the
family-wide tag convention `tag-name = "{{crate_name}}-v{{version}}"` (so tags
are unambiguous across repos and match `scripts/publish-status.py`).

## The current (first) publish closes a large gap

As of this writing every crate is many versions ahead of crates.io (medpdf
0.9.2 → 0.11.0, medpdf-image 0.2.2 → 0.4.3, pdf-maker 0.9.3 → 0.13.1, pdf-dump
0.12.6 → 0.24.0).  crates.io allows version jumps, so the first publish simply
lands the current versions; there is no need to publish the intervening ones.
