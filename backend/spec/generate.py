#!/usr/bin/env python3
"""Generate spec/openapi.yaml from the code, so the contract never drifts:

- paths, methods, auth, query/body/response types: parsed from the Rust HTTP
  layer (rust/crates/api/src/routes/*.rs);
- schemas: JSON Schema of the shared TypeScript contract types
  (webapp/src/lib/api/types.ts), produced by ts-json-schema-generator.

    npx ts-json-schema-generator --path ../../webapp/src/lib/api/types.ts --type '*' \\
        --no-type-check --expose export -o /tmp/schemas.json
    python3 generate.py /tmp/schemas.json
"""
import json, re, sys, pathlib

HERE = pathlib.Path(__file__).parent
ROUTES = HERE.parent / "rust/crates/api/src/routes"
schemas = json.load(open(sys.argv[1]))["definitions"] if len(sys.argv) > 1 else {}

# ---- route table: .route("/path", get(mod::fn).post(mod::fn2))
table = (ROUTES / "mod.rs").read_text()
routes = []
for m in re.finditer(r'\.route\("([^"]+)",\s*((?:\w+\([\w:]+\)\.?)+)\)', table):
    path, chain = m.group(1), m.group(2)
    for verb, handler in re.findall(r'(\w+)\(([\w:]+)\)', chain):
        mod, fn = handler.split("::")
        routes.append((path, verb, mod, fn))

def generic_arg(text, wrapper):
    """The balanced type argument of the first `wrapper<…>` in text."""
    i = text.find(wrapper + "<")
    if i < 0: return None
    i += len(wrapper) + 1
    depth, j = 1, i
    while j < len(text) and depth:
        depth += {"<": 1, ">": -1}.get(text[j], 0)
        j += 1
    return text[i:j - 1].strip()

# ---- handler signatures
handlers = {}
for f in ROUTES.glob("*.rs"):
    src = f.read_text()
    structs = {}
    for sm in re.finditer(r'pub struct (\w+)(?:<\w+>)? \{(.*?)\n\}', src, re.S):
        fields = re.findall(r'^\s+(?:pub )?(\w+): (.+),\s*$', sm.group(2), re.M)
        structs[sm.group(1)] = fields
    for hm in re.finditer(r'pub async fn (\w+)\((.*?)\)\s*->\s*([^{]+)\{', src, re.S):
        name, args, ret = hm.group(1), hm.group(2), hm.group(3)
        h = {"auth": "Caller(" in args, "optional_auth": "MaybeCaller" in args}
        body = generic_arg(args, "Body") if "Body(" in args else None
        if body: h["body"] = body
        if "Multipart" in args: h["multipart"] = True
        q = re.search(r'Q\(\w+\): Q<(\w+)>', args)
        if q: h["query"] = structs.get(q.group(1), [])
        h["response"] = generic_arg(ret, "Json")
        h["status"] = 201 if "StatusCode::CREATED" in src[hm.end():hm.end()+400] else (204 if "StatusCode" in ret and "Json" not in ret else 200)
        h["local_structs"] = structs
        handlers[(f.stem, name)] = h

def schema_ref(t, local, subst=None):
    if t is None: return None
    t = t.strip()
    if subst and t in subst: t = subst[t]
    m = re.fullmatch(r'Option<(.+)>', t)
    if m: return schema_ref(m.group(1), local, subst)
    g = re.fullmatch(r'(\w+)<(\w+)>', t)
    if g and g.group(1) in local:  # generic local struct, e.g. StatusReq<BookingStatus>
        return schema_ref(g.group(1), local, {"T": g.group(2)})
    m = re.fullmatch(r'Vec<(.+)>', t)
    if m: return {"type": "array", "items": schema_ref(m.group(1), local)}
    if t in ("Value",): return {"type": "object"}
    if t == "T" : return {"type": "string"}
    if t in local:
        props = {}
        for fname, ftype in local[t]:
            camel = re.sub(r'_(\w)', lambda x: x.group(1).upper(), fname)
            props[camel] = schema_ref(ftype, local, subst) or {"type": "string"}
        return {"type": "object", "properties": props}
    if t in ("String", "&str"): return {"type": "string"}
    if t in ("i64", "u32", "usize"): return {"type": "integer"}
    if t in ("bool",): return {"type": "boolean"}
    if t in ("f64",): return {"type": "number"}
    if re.fullmatch(r'BTreeMap<.*>', t): return {"type": "object", "additionalProperties": {"type": "string"}}
    return {"$ref": f"#/components/schemas/{t}"}

paths = {}
for path, verb, mod, fn in routes:
    h = handlers.get((mod, fn), {})
    op = {"operationId": fn if fn not in [r[3] for r in routes if r != (path, verb, mod, fn)] else f"{mod}_{fn}", "tags": [mod]}
    params = [{"name": p, "in": "path", "required": True, "schema": {"type": "string"}} for p in re.findall(r'\{(\w+)\}', path)]
    for fname, ftype in h.get("query", []):
        camel = re.sub(r'_(\w)', lambda x: x.group(1).upper(), fname)
        params.append({"name": camel, "in": "query", "required": not ftype.startswith("Option"), "schema": {"type": "string"}})
    if params: op["parameters"] = params
    if h.get("auth"): op["security"] = [{"bearer": []}]
    elif h.get("optional_auth"): op["security"] = [{}, {"bearer": []}]
    if h.get("body"):
        op["requestBody"] = {"required": True, "content": {"application/json": {"schema": schema_ref(h["body"], h["local_structs"])}}}
    if h.get("multipart"):
        op["requestBody"] = {"required": True, "content": {"multipart/form-data": {"schema": {"type": "object", "required": ["requirementId", "file"], "properties": {
            "requirementId": {"type": "string"}, "number": {"type": "string"}, "expiresOn": {"type": "string", "format": "date"}, "file": {"type": "string", "format": "binary"}}}}}}
    status = str(h.get("status", 200))
    resp = {"description": "OK"}
    if h.get("response"): resp["content"] = {"application/json": {"schema": schema_ref(h["response"], h["local_structs"])}}
    if fn == "events": resp = {"description": "Server-Sent Events: ready, notification, change", "content": {"text/event-stream": {"schema": {"type": "string"}}}}
    if fn == "document_file": resp = {"description": "The document file", "content": {"application/octet-stream": {"schema": {"type": "string", "format": "binary"}}}}
    op["responses"] = {status: resp, "default": {"description": "Error", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}}}
    paths.setdefault(path, {})[verb] = op

# Fix $defs refs from ts-json-schema-generator
comp = json.loads(json.dumps(schemas).replace("#/definitions/", "#/components/schemas/"))
comp["Error"] = {"type": "object", "required": ["code", "message"], "properties": {"code": {"type": "string", "example": "VALIDATION"}, "message": {"type": "string"}}}
used = set(re.findall(r'#/components/schemas/(\w+)', json.dumps(paths)))
missing = sorted(u for u in used if u not in comp)
for m in missing:
    comp[m] = {"type": "object", "description": "See backend/rust/crates/domain/src/model.rs"}

spec = {
    "openapi": "3.1.0",
    "info": {"title": "Feast API", "version": "1.0.0", "description": "One deployment per country; every country serves this same contract. Generated by spec/generate.py — do not edit by hand. Money is integer minor units of the market's currency. Errors are {code, message}."},
    "servers": [{"url": "https://au.feast.example", "description": "Australia"}, {"url": "https://in.feast.example", "description": "India"}, {"url": "http://localhost:8080", "description": "Local"}],
    "components": {"securitySchemes": {"bearer": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT"}}, "schemas": comp},
    "paths": dict(sorted(paths.items())),
}
try:
    import yaml
    out = yaml.safe_dump(spec, sort_keys=False, allow_unicode=True, width=120)
    (HERE / "openapi.yaml").write_text("# Generated by spec/generate.py from the Rust routes + TypeScript contract types. Do not edit.\n" + out)
except ImportError:
    (HERE / "openapi.json").write_text(json.dumps(spec, indent=2))
print(f"{len(routes)} operations, {len(paths)} paths, {len(comp)} schemas; placeholder schemas for: {', '.join(missing) or 'none'}")
