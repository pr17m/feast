"""HTTP conformance test: dish photos on any Feast backend serving Australia
with development seed data (admin@feast.demo, owner.au@feast.demo, diner +61412345678).

    python3 menu_photos_flow.py http://localhost:8080          # exit code 1 on any failure

Covers: an owner uploads a photo for a dish; diners see the same photo on the
menu; the public /v1/media URL returns the image with the right type and
caching; only real JPG/PNG/WebP files under 5 MB are accepted; other owners,
diners and Feast staff without support access can't change photos; removing a
stock photo stops Feast adding another; private files can't be reached
through the media route.
FEAST_ADMIN_PASSWORD overrides the seeded password.
"""
import json, os, struct, sys, urllib.request, uuid, zlib

BASE = sys.argv[1].rstrip("/")
PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")
fails = 0


def call(method, path, body=None, token=None, raw=None, ctype=None, full=False):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(BASE + path, data=data, method=method)
    if data is not None:
        req.add_header("content-type", ctype or "application/json")
    if token:
        req.add_header("authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req) as r:
            b = r.read()
            out = json.loads(b) if b and r.headers.get_content_type() == "application/json" else b
            return (r.status, out, r.headers) if full else (r.status, out)
    except urllib.error.HTTPError as e:
        b = e.read()
        try:
            out = json.loads(b or b"{}")
        except ValueError:
            out = {"raw": b.decode(errors="replace")}
        return (e.code, out, e.headers) if full else (e.code, out)


def ok(cond, msg):
    global fails
    print(("PASS " if cond else "FAIL ") + msg)
    if not cond:
        fails += 1


def login(email):
    s, r = call("POST", "/v1/auth/login", {"email": email, "password": PASSWORD})
    assert s == 200, f"login {email}: {s} {r}"
    return r["token"]


def otp(phone):
    call("POST", "/v1/auth/otp", {"phone": phone})
    s, r = call("POST", "/v1/auth/otp/verify", {"phone": phone, "otp": "123456"})
    assert s == 200, f"otp {phone}: {s} {r}"
    return r["token"]


def png(w=2, h=2):
    """A tiny valid PNG."""
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    raw = b"".join(b"\x00" + b"\xd9\x48\x0f" * w for _ in range(h))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def multipart(content, filename="dish.png", ctype="image/png"):
    bnd = "PHOTOBOUNDARY" + uuid.uuid4().hex
    body = (f'--{bnd}\r\nContent-Disposition: form-data; name="file"; filename="{filename}"\r\nContent-Type: {ctype}\r\n\r\n').encode() + content + f"\r\n--{bnd}--\r\n".encode()
    return body, f"multipart/form-data; boundary={bnd}"


def upload(rid, item_id, content, token, **kw):
    body, ct = multipart(content, **kw)
    return call("POST", f"/v1/partner/restaurants/{rid}/menu/items/{item_id}/photo", raw=body, ctype=ct, token=token)


def menu_item(rid, item_id, token=None, partner=False):
    path = f"/v1/partner/restaurants/{rid}/menu" if partner else f"/v1/restaurants/{rid}/menu"
    s, m = call("GET", path, token=token)
    assert s == 200, f"menu {path}: {s} {m}"
    return next(i for c in m["categories"] for i in c["items"] if i["id"] == item_id)


admin, owner, diner = login("admin@feast.demo"), login("owner.au@feast.demo"), otp("+61412345678")
s, mine = call("GET", "/v1/partner/restaurants", token=owner)
rid = mine[0]["id"]
s, m = call("GET", f"/v1/partner/restaurants/{rid}/menu", token=owner)
items = [i for c in m["categories"] for i in c["items"]]
item, other_item = items[0], items[1]
stocked = [i for i in items if i.get("image") and i.get("imageSource", "STOCK") == "STOCK"]
ok(len(stocked) >= len(items) - 3, f"seeded dishes come with stock photos ({len(stocked)} of {len(items)})")

# ------------------------------------------------------------ owner uploads; diners see it
photo = png()
s, it = upload(rid, item["id"], photo, owner)
ok(s == 200 and it.get("imageSource") == "OWNER" and it.get("image", "").startswith("/v1/media/menu/"), f"owner uploads a photo ({s} {it.get('image') if isinstance(it, dict) else it})")
url = it.get("image", "")
seen = menu_item(rid, item["id"])
ok(seen.get("image") == url and seen.get("imageSource") == "OWNER", "diners see the owner's photo on the menu")
s, body, h = call("GET", url, full=True)
ok(s == 200 and body == photo, f"the media URL returns the photo bytes ({s}, {len(body) if isinstance(body, bytes) else body})")
ok(h.get("content-type") == "image/png", f"with its content type ({h.get('content-type')})")
ok("immutable" in (h.get("cache-control") or "") and (h.get("x-content-type-options") == "nosniff"), "cacheable for a year, nosniff")
s, _ = call("GET", url, token=diner)
ok(s == 200, "signed-in diners load it too")

# Replacing gives a new URL (old one keeps working for cached menus).
s, it2 = upload(rid, item["id"], png(3, 3), owner)
ok(s == 200 and it2["image"] != url, "replacing makes a new photo URL")
s, _ = call("GET", it2["image"])
ok(s == 200, "the new photo loads")

# ------------------------------------------------------------ validation
s, e = upload(rid, other_item["id"], b"<html><script>alert(1)</script></html>", owner, filename="x.png")
ok(s in (400, 422) and e.get("code") == "INVALID_FILE_TYPE", f"non-images are refused even when named .png ({s} {e.get('code')})")
s, e = upload(rid, other_item["id"], b"%PDF-1.7 fake", owner, filename="menu.pdf", ctype="application/pdf")
ok(s in (400, 422) and e.get("code") == "INVALID_FILE_TYPE", f"PDFs are refused ({s} {e.get('code')})")
s, e = upload(rid, other_item["id"], b"", owner)
ok(s in (400, 422), f"empty uploads are refused ({s})")
big = png() + b"\x00" * (5 * 1024 * 1024 + 10)
s, e = upload(rid, other_item["id"], big, owner)
ok(s in (400, 413, 422), f"photos over 5 MB are refused ({s} {e.get('code') if isinstance(e, dict) else ''})")
s, e = upload(rid, "no_such_item", photo, owner)
ok(s == 404, f"unknown dish → 404 ({s})")

# ------------------------------------------------------------ who may change photos
# A restaurant this owner doesn't manage (seeded Australian venues).
orid = next((x for x in ["r_pho_republic", "r_laneway_grind", "r_spice_route", "r_swan_grill"] if x not in {m["id"] for m in mine}), None)
s, om = call("GET", f"/v1/restaurants/{orid}/menu")
if s == 200:
    oitem = om["categories"][0]["items"][0]
    s, e = upload(orid, oitem["id"], photo, owner)
    ok(s in (403, 404), f"an owner can't upload to someone else's restaurant ({s})")
else:
    print(f"SKIP no other restaurant ({s})")
s, e = upload(rid, other_item["id"], photo, diner)
ok(s == 403, f"diners can't upload ({s})")
s, e = upload(rid, other_item["id"], photo, admin)
ok(s == 403 and e.get("code") == "SUPPORT_ACCESS_REQUIRED", f"Feast staff need the owner's support access ({s} {e.get('code')})")
s, _ = call("DELETE", f"/v1/partner/restaurants/{rid}/menu/items/{item['id']}/photo", token=admin)
ok(s == 403, "…to remove photos too")
s, acc = call("PUT", f"/v1/partner/restaurants/{rid}/support-access", {"hours": 1}, token=owner)
s, it = upload(rid, other_item["id"], photo, admin)
ok(s == 200 and it.get("imageSource") == "OWNER", f"with support access, staff can upload for the owner ({s})")
call("PUT", f"/v1/partner/restaurants/{rid}/support-access", {"hours": 0}, token=owner)

# ------------------------------------------------------------ removing and stock photos
third = items[2]
s, it = call("DELETE", f"/v1/partner/restaurants/{rid}/menu/items/{third['id']}/photo", token=owner)
ok(s == 200 and not it.get("image") and it.get("stockDeclined") is True, "removing a stock photo leaves the dish without one, and Feast won't add another")
ok(not menu_item(rid, third["id"]).get("image"), "diners then see no photo (generated art)")
s, e = call("POST", f"/v1/partner/restaurants/{rid}/menu/items/{third['id']}/photo/find", token=owner)
ok((s == 409 and e.get("code") == "STOCK_PHOTOS_OFF") or (s == 200 and e.get("imageSource") == "STOCK") or s == 404,
   f"'Find a photo' uses the stock service when configured ({s} {e.get('code') if isinstance(e, dict) else ''})")
s, it = call("DELETE", f"/v1/partner/restaurants/{rid}/menu/items/{item['id']}/photo", token=owner)
ok(s == 200 and not it.get("image") and not it.get("stockDeclined"), "removing your own photo is allowed")

# ------------------------------------------------------------ the media route only serves public photos
for path in ["/v1/media/../documents/x", "/v1/media/documents/r/d", "/v1/media/menu/does/not/exist", "/v1/media/%2e%2e/%2e%2e/etc/passwd"]:
    s, _ = call("GET", path)
    ok(s in (400, 404), f"{path} → {s}")

print("FAILURES:", fails)
sys.exit(1 if fails else 0)
