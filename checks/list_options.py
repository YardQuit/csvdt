"""Does --list-options describe how the binary actually parses?

This list is the contract between csvdt and its Emacs front end, which reads
it to tell an option's value from a filename and never names an option
itself. So a 'separator' of 'either' has to mean both spellings really work,
and 'equals' has to mean the separate spelling really does not -- otherwise
the front end mistakes a value for a file, or refuses a valid run.

Exit status alone cannot answer this: a value left where the filename goes
parses perfectly well and fails later as a missing file, which looks like an
error but is a different answer from the option having taken it. The check
therefore asks whether csvdt reported reading that value as a file.
"""
import os, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")
DATA = b"a,b,c\n2024-06-15T14:00:00Z,1700000000,x\n"

# A value each valued option accepts, so a refusal means the spelling was
# wrong rather than the value.
VALUES = {
    "--insert": "0,after", "--rfc3339": "1", "--utc": "0", "--local": "0",
    "--offset": "0,+02:00", "--split": "0", "--duration": "0",
    "--remove": "1", "--arrange": "1", "--trim": "none",
    "--read-delimiter": ",", "--comment": "#", "--print-delimiter": ";",
    "--quote": "always", "--flexible": "keep",
    # Needs a filling mode to be allowed at all, and writes
    # its account to stderr rather than a file left behind.
    "--fill-log": "-",
}
# -i has nothing to place without an action.
EXTRA = {"--insert": ["-u0"], "--fill-log": ["-f=fix"]}
ASKED = ("--help", "--version", "--list-options")


def options():
    out = subprocess.run([CSVDT, "--list-options"],
                         capture_output=True).stdout.decode()
    for line in out.splitlines():
        if line.startswith("#") or not line.strip():
            continue
        fields = (line.split("\t") + ["", "", "", "", ""])[:5]
        yield fields[0], fields[1], fields[2], fields[3]


def took_the_value(args, value):
    p = subprocess.run([CSVDT] + args, input=DATA, capture_output=True)
    err = p.stderr.decode()
    if f"no file named '{value}'" in err:
        return False, "read as a filename"
    if p.returncode in (2, 100):
        first = err.strip().splitlines()
        return False, (first[0][:60] if first else "refused")
    return True, ""


def main():
    bad = []
    for short, long, value, separator in options():
        if value == "none" or long in ASKED:
            continue
        if long not in VALUES:
            bad.append((long, "no test value known -- a new option?"))
            continue
        v, extra = VALUES[long], EXTRA.get(long, [])
        forms = {"--long=value": [f"{long}={v}"] + extra,
                 "--long value": [long, v] + extra}
        if short:
            forms["-s=value"] = [f"{short}={v}"] + extra
            forms["-svalue"] = [f"{short}{v}"] + extra
            forms["-s value"] = [short, v] + extra

        if separator not in ("either", "equals"):
            bad.append((long, f"unknown separator {separator!r}"))
            continue
        # 'equals' means attached with '=', for the short form too: a bare
        # -fkeep is read as a cluster of short flags.
        must = ({"--long=value", "-s=value"} if separator == "equals"
                else {"--long=value", "--long value", "-svalue", "-s value"})
        for name, args in forms.items():
            ok, why = took_the_value(args, v)
            if name in must and not ok:
                bad.append((long, f"'{separator}' but {name} did not: {why}"))
            if (separator == "equals" and ok
                    and name in ("--long value", "-s value")):
                bad.append((long, f"'{separator}' but {name} took it anyway"))

    print(f"list-options contract: {len(bad)} mismatches")
    for long, detail in bad:
        print(f"  {long}: {detail}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
