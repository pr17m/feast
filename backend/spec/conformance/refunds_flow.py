"""HTTP conformance test for refunds, against any Feast backend serving
Australia with development seed data and test payments (the mock provider):

    python3 refunds_flow.py http://localhost:8080     # exit code 1 on any failure

Covers the refunds policy for upfront table charges and Feast's own refunds:
- a diner who cancels at least 2 hours before the slot gets the charge back in
  full; a later cancellation doesn't (the restaurant keeps it);
- a restaurant's (or Feast's) cancellation always refunds;
- a payment that arrives after its booking was cancelled (signed provider
  webhook) never brings the booking back: it is refunded automatically, once,
  and so is a second payment for an already-paid booking;
- an admin refunds by hand (with a note), never more than what's left;
- counter (offline) payments are never refunded by Feast;
- refunds are visible to the payer (not to other diners), the diner is told,
  final refund states stay final, and every step is in the audit trail.

Upfront charges apply to fine-dining (price band 4) restaurants: Swan River
Grill in Perth. The late-cancellation check temporarily makes another
restaurant fine dining and needs a slot less than 2 hours away, so run the server
under libfaketime at ~18:30 Sydney (see booking_deals_flow.py); otherwise that
check is reported as SKIP. Test webhooks are signed with the mock provider's
secret (FEAST_MOCK_WEBHOOK_SECRET, default as in development).
"""
import hashlib, hmac, json, os, sys, urllib.request, uuid
from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo

BASE = sys.argv[1].rstrip("/")
PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")
OTP = os.environ.get("FEAST_DEMO_OTP", "123456")
SECRET = os.environ.get("FEAST_MOCK_WEBHOOK_SECRET", "mock_webhook_test_secret")
NOW = datetime.fromisoformat(os.environ["FEAST_TEST_NOW"].replace("Z", "+00:00")) if os.environ.get("FEAST_TEST_NOW") else datetime.now(timezone.utc)


def call(method, path, body=None, token=None, raw=None, headers=None):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(BASE + path, data=data, method=method)
    if data is not None:
        req.add_header("content-type", "application/json")
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    if token:
        req.add_header("authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req) as r:
            b = r.read()
            return r.status, (json.loads(b) if b else None)
    except urllib.error.HTTPError as e:
        b = e.read()
        return e.code, (json.loads(b) if b else {})


fails = skips = 0


def ok(cond, msg):
    global fails
    print(("PASS " if cond else "FAIL ") + msg)
    if not cond:
        fails += 1


def skip(msg):
    global skips
    skips += 1
    print("SKIP " + msg)


def login(email):
    s, r = call("POST", "/v1/auth/login", {"email": email, "password": PASSWORD})
    assert s == 200, (email, s, r)
    return r["token"], r["user"]


def diner(name):
    phone = f"+614{uuid.uuid4().int % 10**8:08d}"
    call("POST", "/v1/auth/otp", {"phone": phone})
    s, sess = call("POST", "/v1/auth/otp/verify", {"phone": phone, "otp": OTP, "name": name})
    assert s == 200, (s, sess)
    return sess["token"], sess["user"]


def local_day(tz, days=0):
    return (NOW.astimezone(ZoneInfo(tz)) + timedelta(days=days)).strftime("%Y-%m-%d")


def reserve(token, user, r, date, pick=None):
    """Book a slot (PENDING_PAYMENT when there's an upfront charge). `pick` chooses among available slots."""
    s, avail = call("GET", f"/v1/restaurants/{r['id']}/slots?date={date}&guests=2")
    slots = [sl for sl in (avail.get("slots", []) if s == 200 else []) if sl["available"]]
    slot = next((sl for sl in slots if pick is None or pick(sl)), None)
    if not slot:
        return None
    s, res = call("POST", "/v1/bookings", {
        "restaurantId": r["id"], "date": date, "slotId": slot["id"], "guests": 2,
        "contact": {"name": user.get("name") or "Diner", "phone": user["phone"]}, "idempotencyKey": uuid.uuid4().hex,
    }, token=token)
    assert s == 201, (s, res)
    return res["booking"]


def intent_for(token, bk):
    s, pi = call("POST", "/v1/payments/intents", {"purpose": "BOOKING", "referenceId": bk["id"]}, token=token)
    assert s in (200, 201), (s, pi)
    return pi


def pay(token, bk):
    pi = intent_for(token, bk)
    call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD"}, token=token)
    s, bk = call("GET", f"/v1/bookings/{bk['id']}", token=token)
    return bk, pi


def webhook(event):
    raw = json.dumps(event).encode()
    sig = hmac.new(SECRET.encode(), raw, hashlib.sha256).hexdigest()
    return call("POST", "/v1/webhooks/payments/mock", raw=raw, headers={"x-mock-signature": sig})


def brief(rs):
    return [(x.get("reason"), x.get("status"), x.get("amount")) for x in rs]


def refunds_of(token, intent_id):
    s, rs = call("GET", f"/v1/payments/intents/{intent_id}/refunds", token=token)
    return s, (rs if s == 200 else [])


at, admin = login("admin@feast.demo")
s, swan = call("GET", "/v1/restaurants/swan-river-grill")
assert s == 200 and swan["priceBand"] == 4, (s, swan.get("priceBand"))
PERTH = "Australia/Perth"
tomorrow = local_day(PERTH, 1)

# The restaurant's owner (created by an admin) for the restaurant-cancel check.
s, own = call("POST", "/v1/admin/users", {"name": "Sam Swan", "email": f"sam.{uuid.uuid4().hex[:8]}@feast.test", "role": "OWNER", "restaurantIds": [swan["id"]]}, token=at)
ok(s in (200, 201), f"admin creates swan-river-grill's owner ({s})")
ot, _ = login(own["email"])

# 1. Diner cancels in time (tomorrow): full refund.
dt, du = diner("Rita Refund")
bk = reserve(dt, du, swan, tomorrow)
ok(bk is not None and bk["status"] == "PENDING_PAYMENT" and bk["coverCharge"] > 0, f"fine dining asks for an upfront charge ({bk and bk['status']})")
bk, pi = pay(dt, bk)
ok(bk["status"] == "CONFIRMED" and bk["payment"]["amount"] == bk["coverCharge"], "paid: booking confirmed")
s, rs = refunds_of(dt, pi["id"])
ok(s == 200 and rs == [], "no refunds before cancelling")
s, c = call("POST", f"/v1/bookings/{bk['id']}/cancel", {}, token=dt)
ok(s == 200 and c["status"] == "CANCELLED", f"diner cancels tomorrow's booking ({s})")
s, rs = refunds_of(dt, pi["id"])
r1 = rs[0] if rs else {}
ok(len(rs) == 1 and r1["amount"] == bk["coverCharge"] and r1["reason"] == "CANCELLED_IN_TIME", f"cancelled ≥2 h before: full refund ({brief(rs)})")
ok(r1.get("status") == "SUCCEEDED" and r1.get("providerRefundId"), "the provider accepted the refund")
s, _ = call("POST", f"/v1/bookings/{bk['id']}/cancel", {}, token=dt)
s, rs = refunds_of(dt, pi["id"])
ok(len(rs) == 1, "cancelling again doesn't refund twice")
s, rs_other = refunds_of(diner("Nosy Neighbour")[0], pi["id"])
ok(s in (403, 404), f"another diner can't see the refund ({s})")
s, rs_owner = refunds_of(ot, pi["id"])
ok(s == 200 and len(rs_owner) == 1, f"the restaurant sees it ({s})")
s, notes = call("GET", "/v1/me/notifications", token=dt)
items = notes.get("items", notes) if isinstance(notes, dict) else (notes or [])
# The projector runs asynchronously; give it a moment.
for _ in range(20):
    if any("Refund" in (n.get("title") or "") for n in items):
        break
    import time; time.sleep(0.25)
    s, notes = call("GET", "/v1/me/notifications", token=dt)
    items = notes.get("items", notes) if isinstance(notes, dict) else (notes or [])
ok(any("Refund" in (n.get("title") or "") for n in items), "the diner is told about the refund")

# A late or repeated provider verdict can't move a final refund.
s, r = webhook({"type": "refund.failed", "refundId": r1.get("providerRefundId", "x")})
s, rs = refunds_of(dt, pi["id"])
ok(rs and rs[0]["status"] == "SUCCEEDED", f"refund.failed after success is ignored ({s})")
s, r = webhook({"type": "refund.succeeded", "refundId": "nope"})
ok(s == 401 or s < 500, f"unknown refund ids are harmless ({s})")
s, r = call("POST", "/v1/webhooks/payments/mock", raw=b'{"type":"refund.succeeded","refundId":"x"}', headers={"x-mock-signature": "00"})
ok(s in (400, 401, 403), f"unsigned webhooks are refused ({s})")

# 2. The restaurant cancels: always refunded.
d2, u2 = diner("Owen Owner-cancel")
b2 = reserve(d2, u2, swan, tomorrow)
b2, pi2 = pay(d2, b2)
s, c = call("POST", f"/v1/bookings/{b2['id']}/cancel", {}, token=ot)
ok(s == 200 and c["status"] == "CANCELLED", f"the restaurant cancels ({s} {c.get('code') if isinstance(c, dict) else ''})")
s, rs = refunds_of(d2, pi2["id"])
ok(len(rs) == 1 and rs[0]["reason"] == "CANCELLED_BY_RESTAURANT" and rs[0]["amount"] == b2["coverCharge"], f"restaurant cancellation: full refund ({brief(rs)})")

# 3. A payment that arrives after the booking was cancelled: refunded, booking stays cancelled.
d3, u3 = diner("Lena Late-payment")
b3 = reserve(d3, u3, swan, tomorrow)
pi3 = intent_for(d3, b3)
s, res = call("DELETE", "/v1/me", {"confirm": "DELETE", "otp": OTP}, token=d3)
if s == 400 and res.get("code") in ("OTP_REQUIRED", "BAD_OTP"):
    call("POST", "/v1/auth/otp", {"phone": u3["phone"]})
    s, res = call("DELETE", "/v1/me", {"confirm": "DELETE", "otp": OTP}, token=d3)
ok(s == 200 and res.get("cancelledBookings", 0) >= 1, f"the diner deletes their account while paying ({s} {res})")
s, r = webhook({"type": "payment.succeeded", "providerRef": pi3.get("providerRef") or pi3["id"], "amount": pi3["amount"], "currency": pi3["currency"]})
ok(s == 200, f"the provider's late payment webhook is accepted ({s} {brief([r]) if s == 201 else r})")
s, again = webhook({"type": "payment.succeeded", "providerRef": pi3.get("providerRef") or pi3["id"], "amount": pi3["amount"], "currency": pi3["currency"]})
ok(s == 200, f"…and its retry too ({s})")
s, rs = refunds_of(at, pi3["id"])
ok(len(rs) == 1 and rs[0]["reason"] == "PAID_AFTER_CANCELLATION" and rs[0]["amount"] == pi3["amount"], f"late payment refunded in full, once ({brief(rs)})")
s, bks = call("GET", "/v1/admin/bookings", token=at)
items = bks.get("items", bks) if isinstance(bks, dict) else bks
late = next((x for x in items or [] if x["id"] == b3["id"]), None)
ok(late is not None and late["status"] == "CANCELLED", f"the cancelled booking was not brought back ({late and late['status']})")

# …and a second payment for a booking someone already paid.
d4, u4 = diner("Dora Double")
b4 = reserve(d4, u4, swan, tomorrow)
first, second = intent_for(d4, b4), intent_for(d4, b4)
call("POST", f"/v1/payments/intents/{first['id']}/confirm", {"method": "CARD"}, token=d4)
s, conf = call("POST", f"/v1/payments/intents/{second['id']}/confirm", {"method": "CARD"}, token=d4)
s, b4now = call("GET", f"/v1/bookings/{b4['id']}", token=d4)
ok(b4now["status"] == "CONFIRMED" and b4now["payment"]["intentId"] == first["id"], "the first payment confirmed the booking")
s, rs = refunds_of(d4, second["id"])
ok(len(rs) == 1 and rs[0]["amount"] == second["amount"], f"the second payment is refunded automatically ({brief(rs)})")
s, rs = refunds_of(d4, first["id"])
ok(rs == [], "the first payment is kept")

# 4. Admin refunds by hand: never more than what's left.
paid_amt = first["amount"]
s, e = call("POST", "/v1/admin/refunds", {"intentId": first["id"], "amount": 100, "note": "Goodwill gesture for the wait"}, token=d4)
ok(s == 403, f"diners can't refund ({s})")
s, e = call("POST", "/v1/admin/refunds", {"intentId": first["id"], "amount": 100, "note": "short"}, token=at)
ok(s == 400, f"a note is required ({s} {e.get('code')})")
s, e = call("POST", "/v1/admin/refunds", {"intentId": first["id"], "amount": paid_amt + 1, "note": "Goodwill gesture for the wait"}, token=at)
ok(s in (400, 409), f"can't refund more than was paid ({s} {e.get('code')})")
half = paid_amt // 2
s, r = call("POST", "/v1/admin/refunds", {"intentId": first["id"], "amount": half, "note": "Goodwill gesture for the wait"}, token=at)
ok(s == 201 and r["amount"] == half and r["reason"] == "MANUAL" and r["status"] == "SUCCEEDED", f"admin refunds part ({s} {brief([r]) if s == 201 else r})")
s, e = call("POST", "/v1/admin/refunds", {"intentId": first["id"], "amount": paid_amt - half + 1, "note": "Rest of the charge back"}, token=at)
ok(s in (400, 409), f"…and no more than what's left ({s})")
s, r = call("POST", "/v1/admin/refunds", {"intentId": first["id"], "note": "Rest of the charge back"}, token=at)
ok(s == 201 and r["amount"] == paid_amt - half, f"the rest, without an amount ({s} {r.get('amount')})")
s, e = call("POST", "/v1/admin/refunds", {"intentId": first["id"], "amount": 1, "note": "Rest of the charge back"}, token=at)
ok(s in (400, 409), f"fully refunded: nothing more ({s} {e.get('code')})")
s, all_r = call("GET", "/v1/admin/refunds", token=at)
ok(s == 200 and sum(1 for x in all_r if x["intentId"] == first["id"]) == 2, "admin lists refunds")
s, trail = call("GET", f"/v1/admin/audit?entityType=refund&limit=100", token=at)
items = trail.get("items", []) if isinstance(trail, dict) else []
mine = [x for x in items if first["id"] in (x.get("note") or "")]
ok(s == 200 and {x["action"] for x in mine} >= {"refund.requested", "refund.succeeded"} and all(x["actor"].get("userId") == admin["id"] for x in mine),
   f"refunds are in the audit trail, by the admin ({s} {[x['action'] for x in mine]})")
s, late_trail = call("GET", f"/v1/admin/audit?entityId={pi3['id']}", token=at)
ok(s == 200 and any(x["action"] == "payment.not_applied" for x in late_trail.get("items", [])), "the late payment is audited as not applied")

# 5. A late cancellation keeps the charge (needs a slot < 2 h away). A
#    restaurant is made fine dining for a moment: its (new) owner grants Feast
#    support access, an admin changes the cost for two, and both are undone.
SYD = "Australia/Sydney"
s, cities = call("GET", "/v1/cities")
tz_of = {c["id"]: c["timezone"] for c in cities}
s, rests = call("GET", "/v1/admin/restaurants", token=at)


def soon(tz):
    now_local = NOW.astimezone(ZoneInfo(tz))
    return lambda sl: 0 < (datetime.strptime(f"{local_day(tz)} {sl['time']}", "%Y-%m-%d %H:%M").replace(tzinfo=ZoneInfo(tz)) - now_local).total_seconds() < 2 * 3600 - 60


helpers = [own]
cand = tz = None
for r in rests:
    if r["status"] != "LIVE" or r["id"] == swan["id"] or r.get("cityId") not in tz_of:
        continue
    s, avail = call("GET", f"/v1/restaurants/{r['id']}/slots?date={local_day(tz_of[r['cityId']])}&guests=2")
    if s == 200 and any(sl["available"] and soon(tz_of[r["cityId"]])(sl) for sl in avail.get("slots", [])):
        cand, tz = r, tz_of[r["cityId"]]
        break
if cand is None:
    skip("no restaurant with a slot less than 2 hours away (run under faketime at ~18:30 Sydney)")
else:
    s, helper = call("POST", "/v1/admin/users", {"name": "Hal Helper", "email": f"hal.{uuid.uuid4().hex[:8]}@feast.test", "role": "OWNER", "restaurantIds": [cand["id"]]}, token=at)
    helpers.append(helper)
    ht, _ = login(helper["email"])
    call("PUT", f"/v1/partner/restaurants/{cand['id']}/support-access", {"hours": 1}, token=ht)
    s, e = call("PATCH", f"/v1/admin/restaurants/{cand['id']}", {"costForTwo": swan["costForTwo"]}, token=at)
    try:
        ok(s == 200, f"{cand['slug']} is fine dining for this check ({s} {e.get('code') if isinstance(e, dict) else ''})")
        d5, u5 = diner("Lou Late")
        b5 = reserve(d5, u5, cand, local_day(tz), soon(tz))
        if b5 is None or b5["status"] != "PENDING_PAYMENT":
            skip(f"couldn't book {cand['slug']} with an upfront charge ({b5 and b5['status']})")
        else:
            b5, pi5 = pay(d5, b5)
            s, c = call("POST", f"/v1/bookings/{b5['id']}/cancel", {}, token=d5)
            ok(s == 200 and c["status"] == "CANCELLED", f"diner cancels less than 2 h before ({cand['slug']} {b5['time']})")
            s, rs = refunds_of(d5, pi5["id"])
            ok(s == 200 and rs == [], f"late cancellation: the restaurant keeps the charge ({brief(rs)})")
    finally:
        call("PATCH", f"/v1/admin/restaurants/{cand['id']}", {"costForTwo": cand["costForTwo"]}, token=at)
        call("PUT", f"/v1/partner/restaurants/{cand['id']}/support-access", {"hours": 0}, token=ht)

# 6. Counter payments are never refunded by Feast.
s, hv = call("GET", "/v1/restaurants/harbour-and-vine")
d6, u6 = diner("Cass Counter")
b6 = reserve(d6, u6, hv, local_day(SYD))
if b6 is None:
    skip("no harbour-and-vine slot today: counter payment not refundable")
else:
    if b6["status"] == "PENDING_PAYMENT":
        b6, _ = pay(d6, b6)
    s, q = call("POST", "/v1/bills/quote", {"restaurantId": hv["id"], "billAmount": 4200, "tip": 0, "bookingId": b6["id"]}, token=d6)
    ok(s == 200 and q["lines"][0]["label"] == "Bill total (incl. GST)", f"the typed bill total includes GST ({q.get('lines', [{}])[0].get('label')})")
    s, op = call("POST", f"/v1/bills/{q['id']}/offline-payment", {"method": "CASH"}, token=d6)
    ok(s in (200, 201), f"diner pays at the counter ({s})")
    s, e = call("POST", "/v1/admin/refunds", {"intentId": op["intentId"], "note": "Diner asked for a refund"}, token=at)
    ok(s in (400, 409) and e.get("code") in ("OFFLINE_NOT_REFUNDABLE", "NOT_REFUNDABLE"), f"Feast never refunds counter payments ({s} {e.get('code')})")

# Leave the seed as we found it: our test owners no longer run restaurants.
for h in helpers:
    s, _ = call("PATCH", f"/v1/admin/users/{h['id']}", {"restaurantIds": [], "status": "BLOCKED"}, token=at)
    ok(s == 200, f"test owner {h['email']} retired ({s})")

print(f"\n{'OK' if fails == 0 else 'FAILED'}: {fails} failure(s), {skips} skipped")
sys.exit(1 if fails else 0)
