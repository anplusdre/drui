// The compact forms of POST /patch, expanded into the flat `add` list the patch applies: `children` (nested layers), `templates` with
// `use`, and `repeat` / `each` (copies of a layer or of a whole subtree, with variables per copy). A screen that is eight tiles of an
// icon and a label is one entry, not forty. Everything here is a pure JSON-to-JSON step, applied before the guard and the patch itself.
use serde_json::{Map, Value};

const MAX_ENTRIES: usize = 5000;

pub(crate) fn expand_patch(p: &Value) -> Result<Value, String> {
    let Some(add) = p["add"].as_array() else { return Ok(p.clone()) };
    let templates = p["templates"].as_object().cloned().unwrap_or_default();
    let (mut out, mut auto) = (vec![], 0);
    for e in add {
        expand(e, None, &Map::new(), &templates, &mut out, &mut auto, 0)?;
    }
    let mut q = p.clone();
    if let Some(o) = q.as_object_mut() {
        o.insert("add".into(), Value::Array(out));
        o.remove("templates");
    }
    Ok(q)
}

// `{key}` in a string becomes the variable's value; a string that is only `{key}` keeps the value's own type (a number, a list)
fn subst(v: &Value, vars: &Map<String, Value>, idx: Option<(usize, usize)>) -> Value {
    match v {
        Value::String(s) => {
            let lookup = |key: &str| -> Option<Value> {
                match (key, idx) {
                    ("i", Some((i, _))) => Some(Value::from(i)),
                    ("n", Some((i, _))) => Some(Value::from(i + 1)),
                    _ => vars.get(key).cloned(),
                }
            };
            if let Some(key) = s.strip_prefix('{').and_then(|r| r.strip_suffix('}')).filter(|k| !k.contains(['{', '}'])) {
                if let Some(val) = lookup(key) {
                    return val;
                }
            }
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(a) = rest.find('{') {
                out.push_str(&rest[..a]);
                match rest[a..].find('}') {
                    Some(b) => match lookup(&rest[a + 1..a + b]) {
                        Some(Value::String(t)) => out.push_str(&t),
                        Some(val) => out.push_str(&val.to_string()),
                        None => out.push_str(&rest[a..=a + b]),
                    },
                    None => {
                        out.push_str(&rest[a..]);
                        rest = "";
                        continue;
                    }
                }
                rest = &rest[a + rest[a..].find('}').unwrap_or(0) + 1..];
            }
            out.push_str(rest);
            Value::String(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| subst(x, vars, idx)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), subst(x, vars, idx))).collect()),
        other => other.clone(),
    }
}

// `each` as an object of lists is every combination of one item from each list (Style x Size x State of a component set); an item that
// is an object contributes all its keys, a bare value only its list's name
fn cross(dims: &Map<String, Value>) -> Result<Vec<Map<String, Value>>, String> {
    let lists: Vec<(&String, Vec<Value>)> = dims.iter().map(|(k, v)| (k, v.as_array().cloned().unwrap_or_else(|| vec![v.clone()]))).collect();
    if lists.iter().try_fold(1usize, |n, (_, l)| n.checked_mul(l.len())).is_none_or(|n| n > MAX_ENTRIES) {
        return Err(format!("add: each makes more than {MAX_ENTRIES} copies; split it"));
    }
    Ok(lists.iter().fold(vec![Map::new()], |acc, (k, items)| {
        acc.iter()
            .flat_map(|a| {
                items.iter().map(move |it| {
                    let mut m = a.clone();
                    match it {
                        Value::Object(o) => m.extend(o.clone()),
                        x => {
                            m.insert((*k).clone(), x.clone());
                        }
                    }
                    m
                })
            })
            .collect()
    }))
}

fn expand(entry: &Value, parent_ref: Option<String>, vars: &Map<String, Value>, tpl: &Map<String, Value>, out: &mut Vec<Value>, auto: &mut u32, depth: usize) -> Result<(), String> {
    if depth > 12 {
        return Err("add: layers are nested too deep".into());
    }
    let mut e = entry.as_object().ok_or("add: every entry is a JSON object")?.clone();
    // a template is the entry's base: the entry's own fields win, and its children replace the template's
    for _ in 0..8 {
        let Some(name) = e.remove("use").and_then(|v| v.as_str().map(String::from)) else { break };
        let mut base = tpl.get(&name).and_then(Value::as_object).ok_or_else(|| format!("add: there is no template called {name:?} (templates go next to add)"))?.clone();
        base.extend(e);
        e = base;
    }
    let (repeat, each) = (e.remove("repeat"), e.remove("each"));
    // the variables of each copy: `each` is a list of objects (a bare value becomes {"item": value}); `repeat` alone is n plain copies
    let copies: Vec<Map<String, Value>> = match (&each, &repeat) {
        (Some(Value::Object(dims)), _) => cross(dims)?,
        (Some(Value::Array(a)), _) => a.iter().map(|v| v.as_object().cloned().unwrap_or_else(|| Map::from_iter([("item".to_string(), v.clone())]))).collect(),
        (_, Some(r)) => vec![Map::new(); r["n"].as_u64().or_else(|| r.as_u64()).unwrap_or(1) as usize],
        _ => vec![Map::new()],
    };
    let repeated = each.is_some() || repeat.is_some();
    let n = copies.len();
    for (i, item) in copies.into_iter().enumerate() {
        if out.len() >= MAX_ENTRIES {
            return Err(format!("add: more than {MAX_ENTRIES} layers in one patch; split it"));
        }
        let mut vs = vars.clone();
        vs.extend(item);
        let mut inst = e.clone();
        let children = inst.remove("children");
        let mut inst = match subst(&Value::Object(inst), &vs, repeated.then_some((i, n))) {
            Value::Object(o) => o,
            _ => unreachable!(),
        };
        // a grid of copies: column and row steps added to the first copy's x and y
        if let Some(r) = repeat.as_ref().filter(|r| r.is_object()) {
            let cols = r["cols"].as_u64().unwrap_or(n.max(1) as u64).max(1) as usize;
            let (dx, dy) = (r["dx"].as_f64().unwrap_or(0.0), r["dy"].as_f64().unwrap_or(0.0));
            for (key, step) in [("x", (i % cols) as f64 * dx), ("y", (i / cols) as f64 * dy)] {
                if step != 0.0 {
                    inst.insert(key.into(), Value::from(inst.get(key).and_then(Value::as_f64).unwrap_or(0.0) + step));
                }
            }
        }
        if let Some(pr) = &parent_ref {
            if !inst.contains_key("parent") && !inst.contains_key("parent_ref") {
                inst.insert("parent_ref".into(), Value::String(pr.clone()));
            }
        }
        // a copy's own ref gets its number (tile_0, tile_1); a layer with children and no ref gets one of ours
        let mut my_ref = inst.get("ref").and_then(Value::as_str).map(|r| if repeated { format!("{r}_{i}") } else { r.to_string() });
        if my_ref.is_none() && children.as_ref().is_some_and(|c| c.as_array().is_some_and(|a| !a.is_empty())) {
            *auto += 1;
            my_ref = Some(format!("~{auto}"));
        }
        if let Some(r) = &my_ref {
            inst.insert("ref".into(), Value::String(r.clone()));
        }
        out.push(Value::Object(inst));
        for c in children.as_ref().and_then(Value::as_array).into_iter().flatten() {
            expand(c, my_ref.clone(), &vs, tpl, out, auto, depth + 1)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn adds(p: Value) -> Vec<Value> {
        expand_patch(&p).unwrap()["add"].as_array().unwrap().clone()
    }

    #[test]
    fn children_become_a_flat_list_linked_by_refs() {
        let a = adds(json!({ "add": [{ "kind": "Frame", "name": "Card", "children": [{ "kind": "Text", "text": "Hi" }, { "kind": "Rect" }] }] }));
        assert_eq!(a.len(), 3);
        let r = a[0]["ref"].as_str().unwrap();
        assert!(a[1..].iter().all(|c| c["parent_ref"] == r), "{a:?}");
        assert!(a[0].get("children").is_none());
    }

    #[test]
    fn a_repeat_makes_a_grid_and_numbers_its_refs() {
        let a = adds(json!({ "add": [{ "kind": "Rect", "ref": "tile", "x": 10, "y": 20, "name": "Tile {n}", "repeat": { "n": 8, "cols": 4, "dx": 90, "dy": 100 } }] }));
        assert_eq!(a.len(), 8);
        assert_eq!((a[0]["x"].as_f64(), a[0]["y"].as_f64()), (Some(10.0), Some(20.0)));
        assert_eq!((a[5]["x"].as_f64(), a[5]["y"].as_f64()), (Some(100.0), Some(120.0)), "the sixth is column 1 of row 1");
        assert_eq!(a[7]["name"], "Tile 8");
        assert_eq!(a[3]["ref"], "tile_3");
    }

    #[test]
    fn each_gives_every_copy_its_own_variables_through_the_whole_subtree() {
        let a = adds(json!({
            "templates": { "tile": { "kind": "Frame", "w": 72, "children": [{ "kind": "Ellipse", "fill": "{color}" }, { "kind": "Text", "text": "{label}" }] } },
            "add": [{ "ref": "grid", "kind": "Frame", "children": [{ "use": "tile", "each": [{ "label": "GoRide", "color": "#00aa13" }, { "label": "GoFood", "color": "#ee2737" }] }] }]
        }));
        // grid, then (tile, ellipse, text) twice
        assert_eq!(a.len(), 7, "{a:?}");
        assert_eq!(a[1]["parent_ref"], "grid");
        assert_eq!(a[3]["text"], "GoRide");
        assert_eq!(a[2]["fill"], "#00aa13");
        assert_eq!(a[6]["text"], "GoFood");
        assert_ne!(a[1]["ref"], a[4]["ref"], "the two tiles have different refs");
        assert_eq!(a[5]["parent_ref"], a[4]["ref"], "the second tile's children hang from the second tile");
    }

    #[test]
    fn each_as_an_object_is_every_combination() {
        let a = adds(json!({ "add": [{ "kind": "Frame", "name": "{style}/{size}/{state}", "h": "{h}",
            "each": { "style": ["Primary", "Danger"], "size": [{ "size": "S", "h": 32 }, { "size": "L", "h": 48 }], "state": ["Default", "Hover", "Pressed"] } }] }));
        assert_eq!(a.len(), 12);
        assert!(a.iter().any(|e| e["name"] == "Danger/L/Pressed" && e["h"] == 48), "{a:?}");
        assert!(expand_patch(&json!({ "add": [{ "kind": "Rect", "each": { "a": vec![0; 100], "b": vec![0; 100] } }] })).unwrap_err().contains("split it"));
    }

    #[test]
    fn a_whole_number_variable_keeps_its_type_and_mistakes_are_reported() {
        let a = adds(json!({ "add": [{ "kind": "Rect", "radius": "{r}", "each": [{ "r": 8 }] }] }));
        assert_eq!(a[0]["radius"], 8);
        assert!(expand_patch(&json!({ "add": [{ "use": "nope" }] })).unwrap_err().contains("no template"));
        assert!(expand_patch(&json!({ "add": [{ "kind": "Rect", "repeat": 6000 }] })).unwrap_err().contains("split it"));
        // nothing to expand: the patch comes back as it was
        assert_eq!(expand_patch(&json!({ "set": [{ "id": 1, "fill": "#fff" }] })).unwrap(), json!({ "set": [{ "id": 1, "fill": "#fff" }] }));
    }
}
