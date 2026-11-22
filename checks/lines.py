"""--lines against the full run, cut to the records the lines touch.

The help defines '--lines' by a full run: the output is what a run over the
whole input writes for the records with a line selected, each once and in
input order, plus the header. That definition can be checked without
trusting any of the code behind '--lines', so it is checked here, over
generated input of the awkward kind: quoted fields holding newlines (LF and
CRLF), comment lines, blank lines, lines inside a quoted field that look
like comments or blanks, CRLF line endings, ragged rows, and every dialect
option a reader takes.

Where each record lies comes from two places that do not involve
'--lines'. The line a record starts on is the one the full run reports in
its fill log: every data row is made a different width from the first
record, so every one of them is logged. The number of lines a record runs
over is known from the generator, which wrote the newlines inside its
quoted fields. The first record, which the log measures against rather than
lists, is placed by the generator alone, and the log's lines are held to
the generator's for every other record, so the two routes check each other.

For random sets of lines, the selected run's output is compared with the
full run's records cut to those touching a selected line, its fill log with
the full log's rows for the same records, and its note on stderr with the
lines those records cover.
"""
import os, random, sys, tempfile

import harness

DELIMITERS = [",", ";", "\t", "|"]
COMMENTS = [None, "#", "%"]
TRIMS = ["none", "all", "fields", "headers"]
MODES = ["keep", "fix", "widest"]


def field(rng, dialect):
    """A field as written in the input, and how many newlines it holds."""
    delimiter, quote, comment = dialect
    pieces = rng.choices(
        ["a", "b", "7", " ", delimiter, quote, comment or "c", "\n",
         "\r\n", "\n\n", "\n" + (comment or "c") + " x", "-"],
        weights=[6, 6, 4, 2, 1, 1, 1, 1, 1, 1, 1, 1], k=rng.randint(0, 5))
    value = "".join(pieces)
    special = (delimiter in value or quote in value or "\n" in value
               or "\r" in value or (comment and comment in value)
               or value != value.strip())
    if special or rng.random() < 0.1:
        return quote + value.replace(quote, quote * 2) + quote, value.count("\n")
    return value, 0


def record(rng, dialect, width):
    """A record of WIDTH fields as written, with its count of inner newlines."""
    fields = [field(rng, dialect) for _ in range(width)]
    text = dialect[0].join(text for text, _ in fields)
    # A record of one empty field is an empty line, which the reader skips.
    if text == "":
        text = dialect[1] * 2
    return text, sum(newlines for _, newlines in fields)


def generate(rng, dialect):
    """An input, the first and last line of each record in it, its last line
    and its line ending.

    The first record is the widest, so that every later one differs from it
    and is listed in the fill log under every mode.
    """
    line_ending = rng.choice(["\n", "\r\n"])
    comment = dialect[2]
    widest = rng.randint(2, 5)
    rows = rng.randint(1, 25)
    widths = [widest] + [rng.randint(1, widest - 1) for _ in range(rows)]
    parts, spans, line = [], [], 1

    def filler():
        nonlocal line
        for _ in range(rng.choice([0, 0, 0, 1, 2])):
            if comment and rng.random() < 0.6:
                parts.append(comment + " note" + line_ending)
            else:
                parts.append(line_ending)
            line += 1

    for index, width in enumerate(widths):
        filler()
        text, newlines = record(rng, dialect, width)
        # The last record sometimes ends the input without a line ending.
        last = index == len(widths) - 1 and rng.random() < 0.3
        parts.append(text + ("" if last else line_ending))
        spans.append((line, line + newlines))
        line += newlines + 1
    if parts[-1].endswith(line_ending):
        filler()
    return "".join(parts), spans, line - 1, line_ending


def ranges(lines):
    """LINES, a sorted set of numbers, joined into '--lines' ranges."""
    joined = []
    for line in sorted(lines):
        if joined and joined[-1][1] + 1 >= line:
            joined[-1][1] = max(joined[-1][1], line)
        else:
            joined.append([line, line])
    return joined


def spelled(joined):
    return ",".join(str(a) if a == b else f"{a}-{b}" for a, b in joined)


def selection(rng, last_line):
    """A random '--lines' value and the set of lines it names."""
    parts, named = [], set()
    for _ in range(rng.randint(1, 4)):
        first = rng.randint(1, last_line + 2)
        last = min(first + rng.choice([0, 0, 1, 2, 5]), last_line + 3)
        parts.append(str(first) if first == last else f"{first}-{last}")
        named.update(range(first, last + 1))
    rng.shuffle(parts)
    return ",".join(parts), named


def fill_log(path):
    """The fill log at PATH as (line, row) pairs, without its header."""
    with open(path, encoding="utf-8") as handle:
        rows = handle.read().splitlines()[1:]
    return [(int(row.split(",")[0]), row) for row in rows]


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rng = random.Random(seed)
    findings, inputs, selections = [], 0, 0
    # How much of what the docstring promises the draw reached, so a green
    # line says what it covered.
    spanning = commented = crlf = 0
    with tempfile.TemporaryDirectory() as directory:
        for case in range(250):
            delimiter = rng.choice(DELIMITERS)
            single = rng.random() < 0.3
            quote = "'" if single else '"'
            comment = rng.choice(COMMENTS)
            header = rng.random() < 0.6
            dialect = (delimiter, quote, comment)
            data, spans, last_line, line_ending = generate(rng, dialect)
            spanning += sum(1 for first, last in spans if last > first)
            commented += bool(comment) and (comment + " note") in data
            crlf += line_ending == "\r\n"
            path = harness.written(directory, "in.csv", data)

            args = ["--trim", rng.choice(TRIMS)]
            if delimiter != ",":
                args += ["--read-delimiter", delimiter]
            if single:
                args.append("--single-quote")
            if comment:
                args += ["--comment", comment]
            if header:
                args += ["-H"] + (["-p"] if rng.random() < 0.7 else [])
            mode = rng.choice(MODES)
            args.append("-f" if mode == "keep" else f"-f={mode}")
            if mode != "keep" and rng.random() < 0.3:
                args += rng.choice([["-a", "1,0"], ["--remove", "0"]])

            log = os.path.join(directory, "full.log")
            full = harness.run(args + ["--fill-log", log, path])
            if full.returncode != 0:
                findings.append((case, args, "", "the full run failed: "
                                 + harness.text(full.stderr)[:120]))
                continue
            inputs += 1
            logged = fill_log(log)
            # Every record but the first is listed, the first being the one
            # measured against; under widest, the first is the widest.
            starts = [spans[0][0]] + [line for line, _ in logged]
            if starts != [first for first, _ in spans]:
                findings.append((case, args, "", f"the fill log starts records "
                                 f"on {starts}, the generator on "
                                 f"{[first for first, _ in spans]}"))
                continue
            out = harness.records(harness.text(full.stdout))
            written_header = header and "-p" in args
            body = out[1:] if written_header else out
            data_spans = spans[1:] if header else spans
            data_logged = logged if header else [None] + logged
            if out is None or len(body) != len(data_spans):
                findings.append((case, args, "", "the full output does not "
                                 "split into one record per input record"))
                continue

            for _ in range(6):
                value, named = selection(rng, last_line)
                selections += 1
                touched = [index for index, (first, last) in enumerate(spans)
                           if any(line in named for line in range(first, last + 1))]
                offset = 1 if header else 0
                rows = [body[index - offset] for index in touched if index >= offset]
                want = ([out[0]] if written_header else []) + rows
                want_log = [data_logged[index - offset][1] for index in touched
                            if index >= offset and data_logged[index - offset]]
                taken = ranges({line for index in touched
                                for line in range(spans[index][0], spans[index][1] + 1)})
                widened = any(not all(line in named for line in range(spans[index][0], spans[index][1] + 1))
                              for index in touched)
                if not touched:
                    want_note = ("csvdt: '--lines' took no record: the lines selected hold only "
                                 "comment or blank lines, or lie past the end of the input")
                elif widened:
                    want_note = f"csvdt: '--lines' widened to whole records, taking lines {spelled(taken)}"
                else:
                    want_note = None

                part_log = os.path.join(directory, "part.log")
                part = harness.run(args + ["--fill-log", part_log, "--lines", value, path])
                got = harness.records(harness.text(part.stdout))
                notes = [line for line in harness.text(part.stderr).splitlines()
                         if line.startswith("csvdt: '--lines'")]
                where = f"--lines {value}"
                if part.returncode != 0:
                    findings.append((case, args, where, f"exit {part.returncode}: "
                                     + harness.text(part.stderr)[:120]))
                elif got != want:
                    findings.append((case, args, where, f"wrote {got!r}, want {want!r}"))
                elif [row for _, row in fill_log(part_log)] != want_log:
                    findings.append((case, args, where, "the fill log lists other rows"))
                elif notes != ([want_note] if want_note else []):
                    findings.append((case, args, where, f"said {notes}, want {want_note!r}"))

    print(f"lines: seed {seed}, {inputs} inputs ({spanning} records over "
          f"several lines, {commented} with comment lines, {crlf} with CRLF "
          f"line endings), {selections} selections, {len(findings)} divergences")
    for case, args, where, detail in findings[:8]:
        print(f"  case {case}: {' '.join(args)} {where}")
        print(f"      {detail[:300]}")
    return 1 if findings or not (inputs and spanning and commented and crlf) else 0


if __name__ == "__main__":
    sys.exit(main())
