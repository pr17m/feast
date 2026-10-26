"""HTTP conformance test: trust & safety on any Feast backend serving Australia
with development seed data (admin@feast.demo, owner.au@feast.demo, diner +61412345678).

    python3 trust_flow.py http://localhost:8080          # exit code 1 on any failure

Covers: owners own their content (admins view only, edit with owner-granted
support access, audited); Feast moderation (hidden from diners, kept through
owner edits, appeals); grievance cases both ways (turns, internal notes,
escalation, photos, decisions, access); verified reviews (one per visit,
replies, reports, hiding, rating blend).
FEAST_ADMIN_PASSWORD overrides the seeded password.
"""
import json, os, sys, time, urllib.request, uuid

BASE = sys.argv[1].rstrip("/")
PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")
fails = 0


def call(method, path, body=None, token=None, raw=None, ctype=None):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(BASE + path, data=data, method=method)
    if data is not None:
        req.add_header("content-type", ctype or "application/json")
    if token:
        req.add_header("authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req) as r:
            b = r.read()
            return r.status, (json.loads(b) if b and r.headers.get_content_type() == "application/json" else b)
    except urllib.error.HTTPError as e:
        b = e.read()
        try:
            return e.code, json.loads(b or b"{}")
        except ValueError:
            return e.code, {"raw": b.decode(errors="replace")}


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


def multipart(field, filename, ctype, content):
    bnd = "TRUSTBOUNDARY"
    body = (f'--{bnd}\r\nContent-Disposition: form-data; name="{field}"; filename="{filename}"\r\nContent-Type: {ctype}\r\n\r\n').encode() + content + f"\r\n--{bnd}--\r\n".encode()
    return body, f"multipart/form-data; boundary={bnd}"


admin, owner, diner = login("admin@feast.demo"), login("owner.au@feast.demo"), otp("+61412345678")
stranger = otp("+61412" + str(uuid.uuid4().int)[:6])
s, mine = call("GET", "/v1/partner/restaurants", token=owner)
rid = mine[0]["id"]
slug = mine[0]["slug"]
ok(s == 200 and len(mine) >= 1, f"owner manages {len(mine)} restaurant(s)")

# ------------------------------------------------------------ 1. owners own their content
s, menu = call("GET", f"/v1/partner/restaurants/{rid}/menu", token=owner)
item = menu["categories"][0]["items"][0]
s, _ = call("GET", f"/v1/partner/restaurants/{rid}/menu", token=admin)
ok(s == 200, "admin can view the menu")
s, e = call("PATCH", f"/v1/partner/restaurants/{rid}/menu/items/{item['id']}", {"price": item["price"] + 100}, token=admin)
ok(s == 403 and e.get("code") == "SUPPORT_ACCESS_REQUIRED", f"admin can't edit without support access ({s} {e.get('code')})")
s, e = call("PATCH", f"/v1/partner/restaurants/{rid}/settings", {"bookingsEnabled": True}, token=admin)
ok(s == 403, "admin can't change settings either")
s, e = call("PUT", f"/v1/partner/restaurants/{rid}/support-access", {"hours": 2}, token=admin)
ok(s == 403, "only the owner grants support access")
s, acc = call("PUT", f"/v1/partner/restaurants/{rid}/support-access", {"hours": 2}, token=owner)
ok(s == 200 and acc.get("activeUntil"), "owner grants 2h support access")
s, e = call("PUT", f"/v1/partner/restaurants/{rid}/support-access", {"hours": 500}, token=owner)
ok(s == 400, "support access is capped")
s, it = call("PATCH", f"/v1/partner/restaurants/{rid}/menu/items/{item['id']}", {"price": item["price"] + 100}, token=admin)
ok(s == 200 and it["price"] == item["price"] + 100, "admin edits with support access")
s, acc = call("GET", f"/v1/partner/restaurants/{rid}/support-access", token=owner)
ok(any(a["action"] == "menu.item_updated" and a["actorRole"] == "ADMIN" for a in acc["audit"]), "staff edit is in the owner's audit log")
call("PATCH", f"/v1/partner/restaurants/{rid}/menu/items/{item['id']}", {"price": item["price"]}, token=owner)
s, acc = call("PUT", f"/v1/partner/restaurants/{rid}/support-access", {"hours": 0}, token=owner)
ok(s == 200 and not acc.get("activeUntil"), "owner revokes support access")
s, e = call("PATCH", f"/v1/partner/restaurants/{rid}/menu/items/{item['id']}", {"available": False}, token=admin)
ok(s == 403, "revoked: admin is view-only again")

# ------------------------------------------------------------ 2. moderation
s, rs = call("GET", "/v1/partner/restaurants", token=owner)
offers = [o for o in next(r for r in rs if r["id"] == rid)["offers"] if o["scope"] != "BANK"]
if not offers:  # make sure there's an offer to moderate
    s, _ = call("PUT", f"/v1/partner/restaurants/{rid}/offers", [{"id": "new_1", "scope": "BILL", "title": "10% off food", "percentOff": 10, "terms": []}], token=owner)
    s, rs = call("GET", "/v1/partner/restaurants", token=owner)
    offers = [o for o in next(r for r in rs if r["id"] == rid)["offers"] if o["scope"] != "BANK"]
offer = offers[0]
all_offers = next(r for r in rs if r["id"] == rid)["offers"]
s, e = call("PUT", f"/v1/partner/restaurants/{rid}/offers", [dict(o, title=o["title"] + "!") for o in all_offers], token=admin)
ok(s == 403, "admin can't rewrite the restaurant's own offers")
s, e = call("POST", f"/v1/admin/restaurants/{rid}/moderation", {"kind": "OFFER", "id": offer["id"], "action": "HIDE", "reason": "x"}, token=admin)
ok(s == 400, "hiding needs a real reason")
s, _ = call("POST", f"/v1/admin/restaurants/{rid}/moderation", {"kind": "OFFER", "id": offer["id"], "action": "HIDE", "reason": "Terms don't match the discount shown"}, token=admin)
ok(s == 204, "admin hides an offer")
s, e = call("POST", f"/v1/admin/restaurants/{rid}/moderation", {"kind": "OFFER", "id": offer["id"], "action": "HIDE", "reason": "Terms don't match the discount shown"}, token=owner)
ok(s == 403, "owners can't moderate")
s, pub = call("GET", f"/v1/restaurants/{slug}")
ok(s == 200 and all(o["id"] != offer["id"] for o in pub["offers"]), "hidden offer is gone for diners")
s, rs = call("GET", "/v1/partner/restaurants", token=owner)
mine_offers = next(r for r in rs if r["id"] == rid)["offers"]
ok(any(o["id"] == offer["id"] and o.get("moderation", {}).get("reason") for o in mine_offers), "owner sees it hidden, with the reason")
stripped = [{k: v for k, v in o.items() if k != "moderation"} for o in mine_offers]
s, saved = call("PUT", f"/v1/partner/restaurants/{rid}/offers", stripped, token=owner)
ok(s == 200 and any(o["id"] == offer["id"] and o.get("moderation") for o in saved), "owner's save can't clear moderation")
s, _ = call("POST", f"/v1/partner/restaurants/{rid}/moderation/appeal", {"kind": "OFFER", "id": offer["id"], "note": "Fixed the terms"}, token=owner)
ok(s == 204, "owner asks Feast to review again")
s, q = call("GET", "/v1/admin/moderation", token=admin)
ok(s == 200 and q and q[0]["id"] == offer["id"] and q[0]["moderation"].get("reviewRequestedAt"), "appeal is first in the moderation queue")
s, _ = call("POST", f"/v1/admin/restaurants/{rid}/moderation", {"kind": "OFFER", "id": offer["id"], "action": "RESTORE"}, token=admin)
s, pub = call("GET", f"/v1/restaurants/{slug}")
ok(any(o["id"] == offer["id"] for o in pub["offers"]), "restored offer is back for diners")
s, _ = call("POST", f"/v1/admin/restaurants/{rid}/moderation", {"kind": "MENU_ITEM", "id": item["id"], "action": "HIDE", "reason": "Photo shows a different dish"}, token=admin)
s, pm = call("GET", f"/v1/restaurants/{rid}/menu")
ok(all(i["id"] != item["id"] for c in pm["categories"] for i in c["items"]), "hidden menu item is gone for diners")
call("POST", f"/v1/admin/restaurants/{rid}/moderation", {"kind": "MENU_ITEM", "id": item["id"], "action": "RESTORE"}, token=admin)
s, pm = call("GET", f"/v1/restaurants/{rid}/menu")
ok(any(i["id"] == item["id"] for c in pm["categories"] for i in c["items"]), "restored menu item is back")

# ------------------------------------------------------------ 3. grievances
s, bookings = call("GET", "/v1/me/bookings", token=diner)
owned = {r["id"] for r in mine}
mine_b = [b for b in bookings if b["restaurant"]["id"] in owned and b["status"] in ("COMPLETED", "CONFIRMED", "SEATED", "NO_SHOW")]
ok(len(mine_b) >= 2, f"diner has {len(mine_b)} visits at the owner's restaurants")
past = sorted(mine_b, key=lambda b: b["date"])
b1, b2 = past[-1], past[0]
s, e = call("POST", "/v1/cases", {"bookingId": b1["id"], "reason": "NO_SHOW", "text": "This reason is for restaurants"}, token=diner)
ok(s == 400 and e.get("code") == "INVALID_REASON", "diners pick diner reasons")
s, e = call("POST", "/v1/cases", {"bookingId": b1["id"], "reason": "WRONG_BILL", "text": "short"}, token=diner)
ok(s == 400, "a complaint needs a description")
s, c = call("POST", "/v1/cases", {"bookingId": b1["id"], "reason": "WRONG_BILL", "text": "We were charged for two mains we never ordered."}, token=diner)
ok(s == 201 and c["kind"] == "COMPLAINT" and c["status"] == "AWAITING_RESTAURANT" and c.get("dueAt"), f"diner opens a complaint ({s})")
cid = c["id"]
s, e = call("POST", "/v1/cases", {"bookingId": b1["id"], "reason": "FOOD_QUALITY", "text": "And the food was cold too, sadly."}, token=diner)
ok(s == 409 and e.get("code") == "CASE_EXISTS", "one open complaint per visit")
s, e = call("GET", f"/v1/cases/{cid}", token=stranger)
ok(s == 404, "other diners can't see it")
s, e = call("POST", "/v1/cases", {"bookingId": b1["id"], "reason": "WRONG_BILL", "text": "Not my booking at all, testing"}, token=stranger)
ok(s == 404, "can't complain about someone else's booking")
s, e = call("POST", "/v1/cases", {"bookingId": b1["id"], "reason": "OTHER", "text": "Feast staff shouldn't open cases"}, token=admin)
ok(s == 403, "admins don't open cases")
s, lst = call("GET", f"/v1/partner/restaurants/{b1['restaurant']['id']}/cases", token=owner)
ok(any(x["id"] == cid for x in lst), "restaurant sees the complaint")
s, c = call("POST", f"/v1/cases/{cid}/messages", {"text": "Sorry! We'll refund the two mains."}, token=owner)
ok(s == 200 and c["status"] == "AWAITING_CUSTOMER", "restaurant replies → diner's turn")
s, c = call("POST", f"/v1/cases/{cid}/messages", {"text": "Restaurant has refunded before; check POS", "internal": True}, token=admin)
ok(s == 200 and c["messages"][-1]["internal"], "Feast adds an internal note")
s, e = call("POST", f"/v1/cases/{cid}/messages", {"text": "sneaky internal", "internal": True}, token=diner)
ok(s == 403, "parties can't write internal notes")
s, c = call("GET", f"/v1/cases/{cid}", token=diner)
ok(not any(m["internal"] for m in c["messages"]), "diner never sees internal notes")
s, c = call("GET", f"/v1/cases/{cid}", token=owner)
ok(not any(m["internal"] for m in c["messages"]), "restaurant never sees internal notes")
png = b"\x89PNG\r\n\x1a\n" + b"\x00" * 64
body, ct = multipart("file", "bill.png", "image/png", png)
s, c = call("POST", f"/v1/cases/{cid}/attachments", raw=body, ctype=ct, token=diner)
ok(s == 200 and len(c["attachments"]) == 1, "diner attaches a photo")
body, ct = multipart("file", "evil.png", "image/png", b"<script>alert(1)</script>")
s, e = call("POST", f"/v1/cases/{cid}/attachments", raw=body, ctype=ct, token=diner)
ok(s == 400 and e.get("code") == "INVALID_FILE_TYPE", "non-image upload rejected")
s, f = call("GET", f"/v1/cases/{cid}/attachments/{c['attachments'][0]['id']}", token=owner)
ok(s == 200 and f[:4] == b"\x89PNG", "restaurant opens the photo")
s, f = call("GET", f"/v1/cases/{cid}/attachments/{c['attachments'][0]['id']}", token=stranger)
ok(s == 404, "strangers can't open the photo")
s, c = call("POST", f"/v1/cases/{cid}/escalate", token=diner)
ok(s == 200 and c["status"] == "WITH_FEAST" and c.get("escalatedAt"), "diner escalates to Feast")
s, e = call("POST", f"/v1/cases/{cid}/escalate", token=owner)
ok(s == 409 and e.get("code") == "ALREADY_ESCALATED", "can't escalate twice")
s, desk = call("GET", "/v1/admin/cases", token=admin)
row = next((x for x in desk if x["case"]["id"] == cid), None)
ok(s == 200 and row and row["restaurantComplaints"] >= 1 and any(m["internal"] for m in row["case"]["messages"]), "case is on Feast's desk with history + notes")
s, e = call("GET", "/v1/admin/cases", token=owner)
ok(s == 403, "the desk is Feast-only")
s, e = call("POST", f"/v1/admin/cases/{cid}/resolve", {"decision": "CUSTOMER_BLOCKED", "note": "Not applicable to complaints"}, token=admin)
ok(s == 400 and e.get("code") == "INVALID_DECISION", "decision must fit the case kind")
s, c = call("POST", f"/v1/admin/cases/{cid}/resolve", {"decision": "REFUND_ADVISED", "note": "Bill shows two extra mains; please refund them.", "refundAmount": 1}, token=admin)
ok(s == 200 and c["status"] == "RESOLVED" and c["outcome"]["decision"] == "REFUND_ADVISED", "Feast decides: refund advised")
s, e = call("POST", f"/v1/cases/{cid}/messages", {"text": "One more thing please"}, token=diner)
ok(s == 409 and e.get("code") == "CASE_RESOLVED", "resolved cases are closed")
# restaurant → diner
s, r = call("POST", "/v1/cases", {"bookingId": b2["id"], "reason": "NO_SHOW", "text": "Table for four held 30 minutes, nobody came."}, token=owner)
ok(s == 201 and r["kind"] == "REPORT" and r["status"] == "WITH_FEAST", f"restaurant reports a diner → straight to Feast ({s})")
s, e = call("POST", "/v1/cases", {"bookingId": b2["id"], "reason": "WRONG_BILL", "text": "Restaurants use restaurant reasons"}, token=owner)
ok(s == 400, "restaurants pick restaurant reasons")
s, lst = call("GET", "/v1/me/cases", token=diner)
ok(any(x["id"] == r["id"] for x in lst), "diner sees the report about them")
s, r2 = call("POST", f"/v1/cases/{r['id']}/messages", {"text": "We cancelled by phone that afternoon."}, token=diner)
ok(s == 200 and r2["status"] == "WITH_FEAST", "diner gives their side")
s, e = call("POST", f"/v1/cases/{r['id']}/settle", token=diner)
ok(s == 403, "only Feast closes a report")
s, r2 = call("POST", f"/v1/admin/cases/{r['id']}/resolve", {"decision": "CUSTOMER_WARNED", "note": "First no-show on record; reminder sent."}, token=admin)
ok(s == 200 and r2["outcome"]["decision"] == "CUSTOMER_WARNED", "Feast warns the diner")
# a complaint the diner settles
other = [b for b in mine_b if b["id"] not in (b1["id"], b2["id"])]
if other:
    s, c3 = call("POST", "/v1/cases", {"bookingId": other[0]["id"], "reason": "STAFF_BEHAVIOUR", "text": "Server was rude when we asked for water."}, token=diner)
    s, c3 = call("POST", f"/v1/cases/{c3['id']}/settle", token=diner)
    ok(s == 200 and c3["status"] == "RESOLVED" and c3["outcome"]["decision"] == "SETTLED_BETWEEN_PARTIES", "diner marks a complaint resolved")

# ------------------------------------------------------------ 4. reviews
s, pubr = call("GET", f"/v1/restaurants/{slug}")
before = pubr["ratingCount"]
done = [b for b in bookings if b["status"] == "COMPLETED"]
ok(len(done) >= 1, f"diner has {len(done)} completed visits")
s, mr = call("GET", "/v1/me/reviews", token=diner)
reviewed = {x.get("bookingId") for x in mr} | {(x["restaurant"]["id"], x["visitDate"]) for x in mr}
cand = [b for b in done if b["id"] not in reviewed and (b["restaurant"]["id"], b["date"]) not in reviewed]
b = next((x for x in cand if x["restaurant"]["id"] == rid), cand[0] if cand else None)
ok(b is not None, "an unreviewed completed visit exists")
rslug = b["restaurant"]["slug"]
s, pubr = call("GET", f"/v1/restaurants/{rslug}")
before = pubr["ratingCount"]
s, e = call("POST", "/v1/reviews", {"bookingId": b["id"], "rating": 1, "text": ""}, token=diner)
ok(s == 400, "low ratings need a reason")
s, e = call("POST", "/v1/reviews", {"bookingId": b["id"], "rating": 9, "text": "Great"}, token=diner)
ok(s == 400, "rating must be 1–5")
s, e = call("POST", "/v1/reviews", {"bookingId": b["id"], "rating": 5, "text": "Great"}, token=stranger)
ok(s in (403, 404), "can't review someone else's visit")
s, rv = call("POST", "/v1/reviews", {"bookingId": b["id"], "rating": 5, "text": "Lovely harbour views and the barramundi was perfect."}, token=diner)
ok(s == 201 and rv["status"] == "PUBLISHED" and rv["authorName"].endswith("."), f"diner reviews a completed visit ({s} {rv if s != 201 else ''})")
s, e = call("POST", "/v1/reviews", {"bookingId": b["id"], "rating": 4, "text": "Second go"}, token=diner)
ok(s == 409 and e.get("code") == "ALREADY_REVIEWED", "one review per visit")
s, lst = call("GET", f"/v1/restaurants/{rslug}/reviews")
pr = next((x for x in lst["reviews"] if x["id"] == rv["id"]), None)
ok(s == 200 and pr and "userId" not in pr and lst["summary"]["count"] == before + 1, "public review list (no user ids), count +1")
s, e = call("POST", f"/v1/partner/reviews/{rv['id']}/reply", {"text": "Thanks for coming!"}, token=admin)
ok(s == 403, "admin can't reply for the owner")
s, rep = call("POST", f"/v1/partner/reviews/{rv['id']}/reply", {"text": "Thank you, see you again soon!"}, token=owner)
ok(s == 200 and rep["reply"]["text"].startswith("Thank you"), "owner replies publicly")
s, rep = call("POST", f"/v1/partner/reviews/{rv['id']}/report", {"reason": "This diner never visited on that day"}, token=owner)
ok(s == 200 and rep.get("flag"), "owner reports a review")
s, q = call("GET", "/v1/admin/reviews", token=admin)
ok(any(x["id"] == rv["id"] for x in q), "reported review reaches Feast")
s, h = call("POST", f"/v1/admin/reviews/{rv['id']}/moderate", {"action": "HIDE", "reason": "Visit couldn't be verified by the restaurant"}, token=admin)
ok(s == 200 and h["status"] == "HIDDEN", "Feast hides the review")
s, lst = call("GET", f"/v1/restaurants/{rslug}/reviews")
ok(all(x["id"] != rv["id"] for x in lst["reviews"]) and lst["summary"]["count"] == before, "hidden review leaves the list and the rating")
s, h = call("POST", f"/v1/admin/reviews/{rv['id']}/moderate", {"action": "RESTORE"}, token=admin)
ok(s == 200 and h["status"] == "PUBLISHED" and not h.get("flag"), "Feast restores it and clears the report")

# ------------------------------------------------------------ 5. notifications reach the right people (async worker)
def kinds(tok):
    s, n = call("GET", "/v1/me/notifications?limit=100", token=tok)
    return {x["kind"] for x in n.get("notifications", n.get("items", []))} if isinstance(n, dict) else set()

want = {"owner": (owner, {"CASE_UPDATED", "REVIEW_UPDATED", "CONTENT_MODERATED"}), "diner": (diner, {"CASE_UPDATED", "REVIEW_UPDATED"}), "admin": (admin, {"CASE_UPDATED", "REVIEW_UPDATED", "CONTENT_MODERATED"})}
for name, (tok, need) in want.items():
    got = set()
    for _ in range(20):
        got = kinds(tok)
        if need <= got:
            break
        time.sleep(0.5)
    ok(need <= got, f"{name} notified ({sorted(need & got)})")

print(f"FAILURES: {fails}")
sys.exit(1 if fails else 0)
