// Merging two edited versions of the same layers, given the version they both came from (a three-way merge). It is what live co-editing
// uses when another person's edit arrives while you have edited too, and what merging a branch uses. Layers are matched by id; a
// layer changed on one side only takes that side's change; a layer changed on both sides is merged property by property, and a
// property both sides changed differently keeps yours (and is reported).
use serde_json::{Map, Value};

use crate::model::Shape;

fn object(s: &Shape) -> Map<String, Value> {
    match serde_json::to_value(s) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

// the layer after merging; the names of the properties that clashed (and kept `ours`)
fn merge_one(base: &Shape, ours: &Shape, theirs: &Shape) -> (Shape, Vec<String>) {
    let (b, o, t) = (object(base), object(ours), object(theirs));
    let mut out = Map::new();
    let mut clashes = vec![];
    let keys: std::collections::BTreeSet<&String> = b.keys().chain(o.keys()).chain(t.keys()).collect();
    for k in keys {
        let (bv, ov, tv) = (b.get(k), o.get(k), t.get(k));
        let pick = if ov == bv {
            tv
        } else if tv == bv || ov == tv {
            ov
        } else {
            clashes.push(k.clone());
            ov
        };
        if let Some(v) = pick {
            out.insert(k.clone(), v.clone());
        }
    }
    (serde_json::from_value(Value::Object(out)).unwrap_or_else(|_| ours.clone()), clashes)
}

// merges `ours` and `theirs`, which both started from `base`: the layers (in z order) and a note for each conflict
pub fn merge_shapes(base: &[Shape], ours: &[Shape], theirs: &[Shape]) -> (Vec<Shape>, Vec<String>) {
    let find = |list: &[Shape], id: u64| list.iter().find(|s| s.id == id).cloned();
    let mut notes = vec![];
    let mut merged: Vec<Shape> = vec![];
    for o in ours {
        let (b, t) = (find(base, o.id), find(theirs, o.id));
        match (b, t) {
            (Some(b), Some(t)) => {
                if *o == b {
                    merged.push(t);
                } else if t == b || *o == t {
                    merged.push(o.clone());
                } else {
                    let (m, clashes) = merge_one(&b, o, &t);
                    if !clashes.is_empty() {
                        notes.push(format!("{}: both changed {}; yours was kept", o.name, clashes.join(", ")));
                    }
                    merged.push(m);
                }
            }
            (Some(b), None) => {
                // the other side deleted it: gone, unless you changed it meanwhile
                if *o != b {
                    notes.push(format!("{} was deleted by the other side but you changed it; it was kept", o.name));
                    merged.push(o.clone());
                }
            }
            (None, _) => merged.push(o.clone()), // yours alone (or added on both sides: yours)
        }
    }
    for t in theirs {
        if merged.iter().any(|m| m.id == t.id) {
            continue;
        }
        match find(base, t.id) {
            // you deleted it: it stays deleted, unless the other side changed it meanwhile
            Some(b) if *t == b => {}
            Some(_) => {
                notes.push(format!("{} was deleted by you but the other side changed it; it was kept", t.name));
                merged.push(t.clone());
            }
            None => merged.push(t.clone()), // added on the other side
        }
    }
    // when you did not add, remove or reorder anything, the order is the other side's
    let ids = |l: &[Shape]| l.iter().map(|s| s.id).collect::<Vec<_>>();
    if ids(ours) == ids(base) && ids(&merged).len() == ids(theirs).len() {
        let mut ordered: Vec<Shape> = theirs.iter().filter_map(|t| merged.iter().find(|m| m.id == t.id).cloned()).collect();
        if ordered.len() == merged.len() {
            std::mem::swap(&mut merged, &mut ordered);
        }
    }
    (merged, notes)
}
