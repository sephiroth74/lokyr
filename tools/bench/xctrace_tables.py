"""The tables of an Instruments trace, as `xctrace export` writes them: each
row a dictionary of its columns' mnemonics, each value its text and its
formatted text, the references of the export resolved."""
import subprocess
import xml.etree.ElementTree as ET


def table(trace, schema):
    """The rows of the table of `schema` in the trace at `trace`."""
    xpath = f'/trace-toc/run[@number="1"]/data/table[@schema="{schema}"]'
    out = subprocess.run(
        ["xcrun", "xctrace", "export", "--input", trace, "--xpath", xpath],
        capture_output=True, text=True, check=True,
    ).stdout
    root = ET.fromstring(out)
    columns = [col.findtext("mnemonic") for col in root.iter("col")]
    ids, rows = {}, []
    for row in root.iter("row"):
        values = []
        for cell in row:
            for element in cell.iter():
                if element.get("id"):
                    ids[element.get("id")] = element
            target = ids.get(cell.get("ref"), cell) if cell.get("ref") else cell
            if target.tag == "sentinel":
                values.append(None)
            else:
                values.append((target.text, target.get("fmt")))
        rows.append(dict(zip(columns, values)))
    return rows


def start(trace):
    """The trace's start, as the seconds since the epoch."""
    import datetime
    out = subprocess.run(
        ["xcrun", "xctrace", "export", "--input", trace, "--toc"],
        capture_output=True, text=True, check=True,
    ).stdout
    text = ET.fromstring(out).find("run/info/summary/start-date").text
    return datetime.datetime.fromisoformat(text).timestamp()
