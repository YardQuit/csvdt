# RPM packaging

`csvdt.spec` builds an RPM for Fedora, RHEL and CentOS Stream, following
Fedora's packaging guidelines for an application written in Rust.

Two packages come out of it:

| package | contents |
| --- | --- |
| `csvdt` | the binary, `csvdt.1`, the licence and the docs |
| `emacs-csvdt` | the Emacs front end, with autoloads |

## CI builds it

`.github/workflows/rpm.yml` builds this spec in a `fedora:latest` container —
the only place its macros exist — when a version is tagged, from the monthly
build once that has published, and on a pull request that touches what the
package is built out of.

Only a release is ever packaged. Every run but the pull request check resolves
the highest version tag and builds that, whatever ref the run was triggered
from; a push to `master` produces no package at all. The pull request check is
the one exception, and it builds the change proposed rather than a release,
because asking whether that change still produces a package is the whole point
of running it there. It publishes nothing.

It does more than run `rpmbuild`. The source package is made first and the
`BuildRequires` installed from it, so a requirement the spec forgets fails the
build instead of being met by whatever the image happened to carry. Then
`rpmbuild -ba`, which runs the spec's own `%check`; then `rpmlint` on the spec,
the source package and every binary; then it installs the packages and uses
them — `csvdt` on `PATH`, a conversion, `man -w csvdt` resolved through man's
own search path, and Emacs reaching the front end through the autoloads.
`%files` decides all of that, and a file built but not packaged looks
identical until something installs it.

`rpmlint` is fatal. The two findings this package accepts are written down in
`csvdt.rpmlintrc` with the reason for each, named individually rather than by
switching a check off — so an actual misspelling in `%description` still fails
even though `durations` is excused.

The workflow refuses to build when `Cargo.toml` and this spec disagree on the
version, which would otherwise produce a package named for code it does not
contain.

## The kept generation

One rolling release, holding one build and no history: each run uploads what
it produced and deletes what the previous generation left behind. That
deletion is what keeps it to one — a Fedora bump changes the dist tag in every
file name, so without it the release would collect a copy per Fedora release.

`rpm-release` is the newest version tag. It is rebuilt when a version is
tagged, and again by the monthly build — because the tag does not move but
what a build of it contains does. A new time zone database arrives inside
`chrono-tz`, and a new Fedora changes the dist tag, so an RPM built once at
release time would go on serving the rules and the distribution of the month
it was cut.

A rebuild leaves the package's version-release alone, since the source did
not change. So `dnf install` fetches the current build of the release while
`dnf update` sees nothing new in it. `RPM-BUILD-INFO` is the full account of
which build it is, and `csvdt --version` carries enough of one to tell two
rebuilds apart without the release page in front of you.

It is marked prerelease and never "latest", so it cannot displace the release
GitHub designates current.

### What a version shipped as

The rolling release cannot answer "which packages was 1.0.0 released as" once
1.4.0 exists, because it is replaced. So a version tag also attaches its
packages to its own release, written once and never swept: every real package
name, the source package, the debug packages, `RPM-BUILD-INFO`, and a
`SHA256SUMS-rpms` covering that set. The two version-free copies are left out
— they exist to give the rolling release a name that does not move, and on a
release named for its version they would duplicate a file already there.

That is the release-time build, so its dist tag never moves either: it says
what that version was packaged for, and ages out of installability as Fedora
moves on. `rpm-release` is the copy kept current. Both are useful and they
answer different questions.

### Every build, kept

Neither of those answers the third question, which is the one an analyst comes
back with: *give me the package that produced this result*. The rolling copy is
replaced, and the version's own copy is the release-day build rather than
whichever monthly rebuild was installed at the time.

So every published build is also attached to an archive release named for what
it is — `1.0.0-2026.09.abc1234-tzdata2025b`, the version, the month and commit
it was built from, and the time zone database it carries. That name is read out
of `RPM-BUILD-INFO`, which records what the installed binary actually answered,
rather than assembled from the workflow's own variables: every part of it is
then a fact about the packages being archived.

The monthly build writes the same entry for the binaries, so one entry normally
holds both. Normally, and not always — the packages are compiled inside Fedora
from their own dependency resolution, and can resolve a different `chrono-tz`
from the musl binaries built the same night. The tag is derived from each build
separately for exactly that reason: adopting a name that says `tzdata2025b` for
a package carrying `2025a` is the one confusion the naming exists to prevent.
When the two disagree the month has two entries, which is precisely the case
worth being able to see.

An entry is written once. A rerun producing the same packages finishes what a
failed run left; one producing different packages — a new Fedora, a new
compiler, under a name someone may already have cited — is refused, because
only a person can say which of the two should keep the name.

It costs nothing to keep. GitHub's limits are 2 GiB per file and 1000 assets
per release, with "no limit on the total size of a release, nor bandwidth
usage" — against which one version's packages are about 16 MiB.

### Which Fedora, and for how long

The container is `fedora:latest`, unpinned, so the dist tag follows whatever
Fedora is current on the day of the build: `.fc44` now, `.fc45` the first run
after Fedora 45 becomes the stable release. Nothing here chooses it.

One generation means **one dist tag at a time**. The run that first produces
`.fc45` deletes the `.fc44` packages, because they are assets it did not
produce. Anyone still on the older Fedora loses the download at that moment —
the spec is in the tree and builds against any release it supports, but this
release serves only the current one. Building for two at once is a matrix job,
not something this arrangement gives.

### Scripting it

Every real file name carries the version and the dist tag, and both move, so
each installable package is published under a name that does not:

```bash
sudo dnf install https://github.com/YardQuit/csvdt/releases/download/rpm-release/csvdt.x86_64.rpm
```

`emacs-csvdt.noarch.rpm` is the front end, and optional. It requires
`csvdt = %{version}-%{release}`, so a URL install has to name both packages in
one command — on its own there is no repository for `dnf` to resolve that
dependency from, and it fails rather than pulling the binary in:

```bash
sudo dnf install \
  https://github.com/YardQuit/csvdt/releases/download/rpm-release/csvdt.x86_64.rpm \
  https://github.com/YardQuit/csvdt/releases/download/rpm-release/emacs-csvdt.noarch.rpm
```

For the same reason the two URLs must name the same release: the dependency is
on an exact version-release, so mixing `rpm-release` with a version's own
release page fails as soon as the rolling copy has been rebuilt.

The version-free names are copies of the versioned packages, not different
builds — `SHA256SUMS` covers both, and rpm still reports the real version once
installed.

Those URLs are `releases/download/<tag>`, naming the release. The
`releases/latest/download` form means something else: it follows GitHub's
"Latest" designation, which is the newest version release. That release does
carry packages, but only under their real names — the version-free copies
exist to make a rolling URL work and are not on it — so there is no stable RPM
URL of that form to write down.

For a released version other than the newest, build from this spec against
that tag.

## Building it by hand

Fedora build roots have no network, so the crates are vendored first:

```bash
# From a source tree at the version being packaged
cargo vendor
tar czf ~/rpmbuild/SOURCES/csvdt-1.0.0-vendor.tar.gz vendor/
spectool -g -R packaging/rpm/csvdt.spec     # fetches Source0
rpmbuild -ba packaging/rpm/csvdt.spec
```

`cargo-rpm-macros` (Fedora's `rust-packaging`) provides `%cargo_prep`,
`%cargo_build`, `%cargo_install` and `%cargo_test`. `emacs-filesystem`
provides the site-lisp macros the front-end subpackage installs into.

For getting this into Fedora proper, `rust2rpm` is the maintained route and
generates a spec of this shape from `Cargo.toml`. This one is hand-written and
kept in the tree because it states what the package should contain — treat it
as the reference rather than as the only way to build.

## The man page is generated, not written

`csvdt --generate-man` renders the page from the same clap definitions that
parse a command line, and the spec runs it against the binary it has just
built:

```
"$(find target -type f -path '*/rpm/csvdt')" --generate-man > csvdt.1
```

Found rather than named: `%cargo_build` builds the `rpm` profile and leaves
the binary in `target/rpm`, but a build that passes `--target` nests that under
the triple.

So the page describes the build it came out of. A page kept in the tree would
be a fourth copy of every option — after the clap definitions, `src/help/*.txt`
and the README — and the one that goes stale. `--list-options` exists for the
same reason.

clap_mangen puts everything after the options into a section it calls `EXTRA`.
The three topics in there carry their own headings, so those are promoted to
real sections: the page ends `TIMESTAMP FORMATS`, `KNOWN LIMITS`,
`EXIT STATUS`. `tests/cli.rs` renders the page and looks for each of them, so a
heading renamed in `src/help/` without the list in `src/main.rs` being updated
fails a test rather than quietly losing a section.

The spec's `%check` repeats those greps against the page it just rendered,
which is the version that matters to a package.

## What has been verified, and what has not

The spec was written where no Fedora was reachable, and the packaging notes
said so: it had never been built, only parsed against stubbed macros. Two
defects came out of that parsing — a macro left unescaped in a comment, where
rpm expands it anyway, and a `%changelog` whose weekday did not match its
date, which rpmlint calls an error.

Building it in Fedora found what no amount of parsing would have:

- `emacs-filesystem` owns the site-lisp directories but ships **no rpm macros
  naming them**, so `%{_emacs_sitelispdir}` and `%{_emacs_sitestartdir}` were
  undefined and `%files` failed with `File must begin with "/"` over the
  unexpanded text. On Fedora 44 that package is five directories and nothing
  else. See the `%global` block at the top of the spec.
- The `Requires: emacs-filesystem >= %{_emacs_version}` pin was wrong anyway:
  that pin is for byte-compiled `.elc`, tied to the Emacs that compiled it.
  This ships `csvdt.el` as source.
- `emacs -Q` would have made the front-end check pass while proving nothing,
  since `-Q` implies `--no-site-file` and the autoloads live in site-start.
- The filters excusing rpmlint's false positives, written as a TOML `Filters`
  list, were accepted and reported as loaded and did **nothing**. Hence the
  unfiltered run that is required to fail.

It now builds green on Fedora 44, and the run says so in its own words:

```
Wrote: .../csvdt-1.0.0-1.fc44.src.rpm
Wrote: .../emacs-csvdt-1.0.0-1.fc44.noarch.rpm
Wrote: .../csvdt-1.0.0-1.fc44.x86_64.rpm
test result: ok. 251 passed; 0 failed; 1 ignored          # the spec's %check
/usr/share/man/man1/csvdt.1.gz                            # man -w csvdt
CSVDT(1)              General Commands Manual              CSVDT(1)
Loading /usr/share/emacs/site-lisp/site-start.d/csvdt-init.el (source)...
front end resolves csvdt to: /usr/sbin/csvdt
and gets: csvdt 1.0.0+2026.08.<commit> (IANA tzdata 2025b)
```

`/usr/sbin/csvdt` is `%{_bindir}/csvdt`: Fedora merged `sbin` into `bin`, and
`/usr/sbin` comes first on root's `PATH`.

The `+` is build metadata, elided above the way the paths are. The workflow
sets `CSVDT_BUILD_METADATA` before `rpmbuild`, so the packaged binary names
the month and the commit it was built from — two RPMs of one release differ
in their time zone database and their Fedora, and without it they said the
same thing. It is metadata rather than a version: the package's own
`Version:` stays `1.0.0`, and `rpm -q csvdt` is unchanged.

What each run checks is listed under "CI builds it" above. What it does
**not** check:

- **RHEL and CentOS Stream.** The spec targets them and CI builds only
  Fedora. Their `emacs-filesystem`, and the version of `cargo-rpm-macros`
  they carry, differ.
- **Anything but x86_64.** `ExclusiveArch: %{rust_arches}` claims far more.
- **`mock`.** CI builds in a container with the BuildRequires installed into
  it, which is close to a clean build root but is not one; a package that
  build-requires something the image already had would pass here.
- **That the licence tag is right.** `%{cargo_license}` generates
  `LICENSE.dependencies` from the vendored set at build time, and nothing
  compares it against the `License:` line.

## RHEL and CentOS

`cargo-rpm-macros` comes from EPEL on RHEL 9 and CentOS Stream 9. RHEL 8 ships
an older Rust toolchain than this crate's `edition = "2024"` needs, so it is
not a target: build from source there, or use the static musl binary from the
project's releases.
