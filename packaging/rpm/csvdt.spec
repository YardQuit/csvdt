# Built with Fedora's Rust macros, which is what an application written in
# Rust is expected to use for review.
#
# csvdt is a leaf application rather than a library, so Fedora permits its
# dependencies to be built in from a vendored tarball. The macros below are
# what makes that reviewable: %%cargo_license_summary and
# %%cargo_vendor_manifest produce the licence breakdown and the crate manifest
# a reviewer reads, and they are generated from Cargo.lock at build time
# rather than written here, so they cannot fall behind it. They are doubled
# here because rpm expands macros inside comments too.
#
# Building it:
#
#     spectool -g csvdt.spec          # or fetch Source0 by hand
#     cargo vendor                    # in an unpacked source tree
#     tar czf csvdt-1.0.0-vendor.tar.gz vendor/
#     rpmbuild -ba csvdt.spec
#
# rust2rpm is the maintained route for getting a Rust package into Fedora
# proper. This spec is the hand-written equivalent, kept in the tree because
# it states what the package should contain.

%global crate csvdt
%global forgeurl https://github.com/YardQuit/csvdt

# emacs-filesystem owns the site-lisp directories but ships no rpm macros
# naming them -- on Fedora 44 it is five directories and nothing else, so
# %%{_emacs_sitelispdir} and %%{_emacs_sitestartdir} are undefined in a build
# root that has only it, and %%files then fails with "File must begin with /"
# over the unexpanded macro. Defined here only when they are missing, and
# from the directories emacs-filesystem itself owns, so this package installs
# into paths another package is responsible for creating. Where the macros do
# exist they win, so a release that puts site-lisp somewhere else is obeyed
# rather than overridden.
%{!?_emacs_sitelispdir:  %global _emacs_sitelispdir  %{_datadir}/emacs/site-lisp}
%{!?_emacs_sitestartdir: %global _emacs_sitestartdir %{_emacs_sitelispdir}/site-start.d}

Name:           csvdt
Version:        1.0.0
Release:        1%{?dist}
Summary:        Parse CSV files and convert the timestamps in them

# csvdt itself is GPL-3.0-or-later. The crates built into it are permissive
# and are listed, with their licences, in the LICENSE.dependencies file this
# build generates and ships.
License:        GPL-3.0-or-later
URL:            %{forgeurl}
Source0:        %{forgeurl}/archive/%{version}/%{crate}-%{version}.tar.gz
Source1:        %{crate}-%{version}-vendor.tar.gz

BuildRequires:  cargo-rpm-macros >= 24
# Defines %%{_emacs_sitelispdir}, %%{_emacs_sitestartdir} and %%{_emacs_version}
# for the front-end subpackage below.
BuildRequires:  emacs-filesystem
# %%check renders the page and asks groff whether it is valid roff. The page
# is generated from the option definitions, so nothing upstream of the build
# can answer that: a help file gaining a character roff cannot read produces
# a page that still has every section and still renders wrongly.
BuildRequires:  groff-base
# The man page is rendered by the binary this build produces, so there is no
# help2man here and no page kept in the tree to fall behind the options.

# Rust is not built for every architecture Fedora ships.
ExclusiveArch:  %{rust_arches}

%description
csvdt reads and writes CSV, and converts the timestamps inside it: a Unix
epoch to RFC3339, an RFC3339 timestamp to UTC or to an explicit offset or to
a named time zone, a timestamp split into date and time, and durations
between rows or between two columns of one row.

Durations are reported only in days, hours, minutes and seconds -- never
years, months or weeks, which have no fixed length and would mean rounding to
some average. Nothing here approximates a duration, which matters where a
result may be relied on as evidence.

Time zone rules come from an IANA database compiled into the binary rather
than the host's, so the same TZ gives the same answer wherever it runs.
'csvdt --version' reports which release is built in.

%package -n emacs-csvdt
Summary:        Emacs front end for csvdt
BuildArch:      noarch
Requires:       %{name} = %{version}-%{release}
# Unversioned, where an Emacs add-on usually pins %%{_emacs_version}: that
# pin exists for byte-compiled .elc files, which are tied to the Emacs they
# were compiled by. This ships csvdt.el as source, so any Emacs that can read
# it can load it -- the package declares its own floor of 27.1 in the file
# and has a check that holds it to it. What it needs from emacs-filesystem is
# the directories, which do not move with the Emacs version.
Requires:       emacs-filesystem

%description -n emacs-csvdt
Runs csvdt over an Emacs buffer, a region, or a set of lines banked by
another package, and shows the result in a second buffer.

The help it shows and the arguments it completes come from the binary itself,
so a csvdt that gains an option is usable from Emacs with no change to this
package. Three options are named, and only because a region run cannot be
worked out without knowing which flag says the first record is a header, and
which two set the quote and the delimiter that decide where a record begins.

%prep
%autosetup -n %{crate}-%{version} -a1
# Points cargo at the vendored crates rather than the network, which is what
# a build root requires.
%cargo_prep -v vendor

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%{cargo_vendor_manifest}

# Rendered by the binary just built, from the same option definitions that
# parse a command line -- so the page describes this build and no other.
#
# Found rather than named: %%cargo_build builds the 'rpm' profile, which
# leaves the binary in target/rpm, but a build that passes --target nests
# that under the triple. Failing here with a sentence beats %%files failing
# later with a missing man page.
csvdt_built=$(find target -type f -path '*/rpm/%{crate}' | head -n1)
if [ -z "$csvdt_built" ]; then
    echo "cannot find the binary %%cargo_build produced; no page to render" >&2
    find target -maxdepth 3 -type d >&2
    exit 1
fi
"$csvdt_built" --generate-man > %{crate}.1

%install
%cargo_install
install -Dpm0644 %{crate}.1 %{buildroot}%{_mandir}/man1/%{crate}.1

install -Dpm0644 emacs/csvdt.el %{buildroot}%{_emacs_sitelispdir}/%{crate}.el
install -dm0755 %{buildroot}%{_emacs_sitestartdir}
cat > %{buildroot}%{_emacs_sitestartdir}/%{crate}-init.el <<'EOF'
;; Autoloads only. Requiring the package outright would cost every Emacs
;; start-up the load, for a tool most sessions never reach for.
(autoload 'csvdt-run-dwim "csvdt" "Run csvdt on the banked lines and region." t)
(autoload 'csvdt-run-buffer "csvdt" "Run csvdt on the whole buffer." t)
(autoload 'csvdt-run-region "csvdt" "Run csvdt on the region." t)
(autoload 'csvdt-run-banked "csvdt" "Run csvdt on the banked lines." t)
(autoload 'csvdt-describe-run "csvdt" "Say what a run would cover." t)
(autoload 'csvdt-help "csvdt" "Show csvdt's own --help." t)
(autoload 'csvdt-version "csvdt" "Show which csvdt is installed." t)
EOF

%check
%cargo_test

# Valid roff, not merely bytes. -z produces no output and only diagnostics,
# -ww asks for all of them, and any diagnostic at all fails the build --
# groff warns rather than errors on most of what it cannot read, so its
# silence is the only thing worth accepting.
groff_says=$(groff -ww -man -Tutf8 -z %{crate}.1 2>&1)
if [ -n "$groff_says" ]; then
    echo "the rendered page is not clean roff:" >&2
    echo "$groff_says" >&2
    exit 1
fi
# The page has to be a man page rather than merely bytes: the header a reader
# needs, and the sections the help promises.
grep -q '^\.TH CSVDT 1' %{crate}.1
# The hand-written ones are in the list too: they are spliced in rather than
# rendered, so a page that lost them would still look well-formed.
for section in NAME SYNOPSIS DESCRIPTION OPTIONS EXAMPLES 'TIMESTAMP FORMATS' \
               'KNOWN LIMITS' 'EXIT STATUS' ENVIRONMENT FILES NOTES \
               'SEE ALSO'; do
    grep -q "^\.SH ${section}\$" %{crate}.1 || {
        echo "the rendered page has no ${section} section" >&2
        exit 1
    }
done
# The headings inside KNOWN LIMITS are subsections, not text: the help leaves
# no blank line under a heading, so one that failed to be promoted is filled
# into the paragraph below and the section becomes a wall of prose with
# capitals in it. Counted rather than named, since the help may gain one.
subsections=$(grep -c '^\.SS ' %{crate}.1)
if [ "$subsections" -lt 15 ]; then
    echo "the rendered page has only ${subsections} subsections; headings" >&2
    echo "inside a section have been left as text for roff to fill" >&2
    exit 1
fi

%files
%license LICENSE LICENSE.dependencies
%doc README.md CHANGELOG.md cargo-vendor.txt
%{_bindir}/%{crate}
%{_mandir}/man1/%{crate}.1*

%files -n emacs-csvdt
%license LICENSE
%doc emacs/README.md
%{_emacs_sitelispdir}/%{crate}.el
%{_emacs_sitestartdir}/%{crate}-init.el

%changelog
* Fri Aug 21 2026 Michael A Jones <yardquit@pm.me> - 1.0.0-1
- Initial package.
- csvdt reads CSV, converts the timestamps in it, and writes canonical CSV
  back. Durations are reported only in days, hours, minutes and seconds,
  never in weeks, months or years, because those have no fixed length and a
  duration expressed in them has been silently rounded.
- -l/--local resolves zone names against the IANA database compiled into the
  binary rather than the one installed on the machine, so the same input
  converts identically on any host. 'csvdt --version' names the database it
  carries, and this package is rebuilt monthly so that it does not go stale.
- Installs /usr/bin/csvdt and its manual page, which %build renders from the
  binary itself rather than shipping a copy that can drift from it.
- emacs-csvdt is a separate and optional subpackage carrying the Emacs front
  end, autoloaded from site-start rather than costing every start-up a load.
