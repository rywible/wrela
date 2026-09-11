"""Portable HTML reports with actual native-render captures and replay commands."""
import html
import json
from pathlib import Path
import shutil
from runtime import write_json


def report(directory, result):
    write_json(directory / 'result.json', result)
    esc = lambda value: html.escape(str(value))
    cards = []
    for item in result.get('runs', []):
        title = item.get('test', {}).get('name', item.get('name', 'Check'))
        status = 'PASS' if item.get('passed') else 'FAIL'
        cards.append(f'<article><h2>{esc(status)} · {esc(title)}</h2><p>{esc(item.get("failure", ""))}</p>')
        if item.get('artifact'):
            cards.append(f'<a href="{esc(item["artifact"])}">Trace, seed and saved states</a><pre>{esc(item.get("replayCommand", ""))}</pre>')
        for capture in item.get('captures', []):
            image_path = Path(capture['path'])
            relative = image_path.relative_to(directory) if image_path.is_absolute() else image_path
            cards.append(f'<figure><a href="{esc(relative)}"><img src="{esc(relative)}"></a><figcaption>{esc(capture.get("label", relative.stem))}</figcaption></figure>')
            if capture.get('comparison'):
                comparison=capture['comparison']
                cards.append('<p>Mean RGB error: '+esc(comparison['meanAbsoluteError'])+'</p>')
                for label,key in [('Accepted baseline','expected'),('Difference ×5','diff')]:
                    path=Path(comparison[key]).relative_to(directory)
                    cards.append(f'<figure><img src="{esc(path)}"><figcaption>{label}</figcaption></figure>')
        cards.append('</article>')
    other = {k:v for k,v in result.items() if k != 'runs'}
    content = '<!doctype html><meta charset="utf-8"><title>Wrela test report</title><style>body{background:#161b22;color:#e4e8ed;font:15px system-ui;margin:32px;max-width:1200px}article{background:#222a34;padding:20px;margin:16px 0;border-radius:12px}img{max-width:100%}figure{margin:16px 0}a{color:#90c5ff}pre{white-space:pre-wrap;overflow-wrap:anywhere}h1,h2{font-weight:550}</style>'
    content += f'<h1>Wrela · {esc(result.get("kind", "tests"))} · {"PASS" if result.get("passed") else "FAIL"}</h1>'
    content += '<p>' + esc(result.get('error','')) + '</p>'
    content += ''.join(cards) + '<details><summary>Environment and measurements</summary><pre>' + esc(json.dumps(other, indent=2)) + '</pre></details>'
    (directory / 'index.html').write_text(content)


def images_compare(actual, expected, output):
    import subprocess
    from runtime import ROOT
    result = subprocess.run([str(ROOT / '.build/release/ImageCompare'), str(actual), str(expected), str(output)], text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(result.stderr.strip())
    return json.loads(result.stdout)
