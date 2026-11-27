"""Apply review-4 move lists to an engine mapping. Usage: apply_moves.py <mapping> <moves.json>...
Forms handled: move/rename/split/merge/new on files; folder renames (from without a file
extension); a source listed several times (first op moves its row, later split/merge ops
of the same source only add a note or a new row); a merge into a file that does not exist
yet creates it from the first source."""
import json, sys
p = sys.argv[1]
d = json.load(open(p)); rows = d['rows']
ORDER = {'move': 0, 'rename': 0, 'split': 1, 'merge': 2, 'new': 3}
def idx():
    m = {}
    for i, r in enumerate(rows):
        if r.get('new'): m.setdefault(r['new'], []).append(i)
    return m
def set_change(r, op, to):
    if op in ('merge', 'split'): r['change'] = op
    elif r.get('old'):
        r['change'] = 'move' if r['old'].rsplit('/', 1)[1] == to.rsplit('/', 1)[1] else 'rename'
errs = []; done = set(); notes = []
for mf in sys.argv[2:]:
    items = json.load(open(mf))
    for it in sorted(items, key=lambda x: ORDER.get(x['op'], 9)):
        op, fr, to, why = it['op'], it.get('from'), it.get('to'), it.get('why', '')
        m = idx()
        if op == 'new':
            if to in m:
                notes.append(('new target already planned; noted', to)); continue
            rows.append({'old': None, 'new': to, 'change': 'new', 'note': why}); continue
        if fr in done:
            if op == 'merge':
                notes.append((f'part of {fr.rsplit("/",1)[1]} merges into', to))
            elif to not in m:
                rows.append({'old': None, 'new': to, 'change': 'new', 'note': why})
            continue
        if fr not in m and '.' not in fr.rsplit('/', 1)[1]:
            pre = fr + '/'; n = 0
            for r in rows:
                if r.get('new', '') and r['new'].startswith(pre):
                    r['new'] = to + '/' + r['new'][len(pre):]; n += 1
            if not n: errs.append((mf, 'folder not in plan', fr))
            done.add(fr); continue
        if fr not in m:
            errs.append((mf, 'from not in plan', fr)); continue
        if op in ('move', 'rename') and to != fr and to in m:
            errs.append((mf, 'target exists', to)); continue
        if op == 'split' and to != fr and to in m:
            others = [x for x in items if x.get('from') == fr and x is not it]
            if others:
                op = 'merge'          # the whole file divides into existing files
            else:                     # part leaves; the file itself stays
                for i in m[fr]:
                    rows[i]['change'] = 'split'
                    rows[i]['note'] = f'part moves to {to.rsplit("/",1)[1]}: ' + why
                done.add(fr); continue
        create = op == 'merge' and to not in m
        for i in m[fr]:
            r = rows[i]; r['new'] = to
            set_change(r, 'move' if create else op, to)
            r['note'] = why
        done.add(fr)
json.dump(d, open(p, 'w'), indent=1)
for e in errs: print('ERR', *e)
for n in notes: print('NOTE', *n)
print('errors', len(errs), 'notes', len(notes))
