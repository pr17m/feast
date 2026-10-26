"""HTTP conformance test for arriving at a restaurant on any Feast backend
serving Australia with development seed data:

    python3 arrival_qr_flow.py http://localhost:8080      # exit code 1 on any failure

Covers: the restaurant's printed QR (`/v1/qr/{code}`, also old table codes)
opens the restaurant; owners read their own QR code; table ordering, table
sessions and the table PIN are gone; a booked diner checks in ("I'm here")
and pays the bill with the deal through the QR path; the restaurant sees the
arrival; a walk-in can't pay on Feast (BOOKING_REQUIRED).

The "booked diner" checks need a slot that is bookable now (at least 30
minutes ahead) and already inside its 30-minute check-in window: that happens
just after :00 or :30 in Sydney during lunch or dinner. Run the server under
libfaketime at such an instant and pass the same instant here, e.g. 19:30
Sydney time:

    TZ=UTC faketime -f '@2026-10-25 08:30:00' feast --country au all
    FEAST_TEST_NOW=2026-10-25T08:30:00Z python3 arrival_qr_flow.py http://localhost:8092

Without a slot in reach those checks are reported as SKIP.
"""
import json, os, sys, urllib.request, uuid
from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo

BASE = sys.argv[1].rstrip("/")
ADMIN_PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")
NOW = datetime.fromisoformat(os.environ["FEAST_TEST_NOW"].replace("Z", "+00:00")) if os.environ.get("FEAST_TEST_NOW") else datetime.now(timezone.utc)


def call(method, path, body=None, token=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(BASE + path, data=data, method=method)
    if data is not None:
        req.add_header("content-type", "application/json")
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


def diner(phone, name):
    s, _ = call("POST", "/v1/auth/otp", {"phone": phone})
    s, sess = call("POST", "/v1/auth/otp/verify", {"phone": phone, "otp": "123456", "name": name})
    assert s == 200, (s, sess)
    return sess["token"], sess["user"]


s, admin = call("POST", "/v1/auth/login", {"email": "admin@feast.demo", "password": ADMIN_PASSWORD})
assert s == 200, admin
at = admin["token"]
s, pho = call("GET", "/v1/restaurants/pho-republic")
s, fitz = call("GET", "/v1/restaurants/fitzroy-fire")

# 1. The restaurant's QR code
s, qr = call("GET", f"/v1/partner/restaurants/{pho['id']}/qr", token=at)
ok(s == 200 and qr["restaurant"]["id"] == pho["id"] and qr["code"], f"staff read the restaurant's QR code ({s})")
code = qr["code"]
s, fqr = call("GET", f"/v1/partner/restaurants/{fitz['id']}/qr", token=at)
s, owner = call("POST", "/v1/auth/login", {"email": "owner.au@feast.demo", "password": ADMIN_PASSWORD})
if s == 200:
    owned = owner["user"].get("restaurantIds", [])
    other = next((r for r in (pho["id"], fitz["id"]) if r not in owned), None)
    if other:
        s, e = call("GET", f"/v1/partner/restaurants/{other}/qr", token=owner["token"])
        ok(s in (403, 404), f"an owner can't read another restaurant's QR ({s})")
s, r = call("GET", f"/v1/qr/{code}")
ok(s == 200 and r["restaurant"]["slug"] == "pho-republic" and "pin" not in json.dumps(r).lower(), "a guest's scan opens the restaurant (no PIN anywhere)")
s, r = call("GET", f"/v1/qr/{code.upper()}")
ok(s == 200 and r["restaurant"]["id"] == pho["id"], "codes are case-insensitive")
s, r = call("GET", f"/v1/qr/{code.lower()}-t7")
ok(s == 200 and r["restaurant"]["id"] == pho["id"], "an old table code (code-t7) still opens the restaurant")
s, e = call("GET", "/v1/qr/zzzz9999")
ok(s == 404 and e.get("code") == "QR_NOT_FOUND", "an unknown code is 404 QR_NOT_FOUND")
s, e = call("GET", "/v1/qr/..%2Fx")
ok(s == 404, "a malformed code is 404")

# 2. Table ordering is gone
for method, path in [("GET", f"/v1/tables/{code}-t1"), ("POST", f"/v1/tables/{code}-t1/sessions"), ("GET", "/v1/sessions/ts_x"),
                     ("POST", "/v1/sessions/ts_x/orders"), ("GET", f"/v1/partner/restaurants/{pho['id']}/orders"),
                     ("GET", f"/v1/partner/restaurants/{pho['id']}/tables"), ("POST", f"/v1/partner/restaurants/{pho['id']}/tables/rotate-pin")]:
    s, _ = call(method, path, {} if method == "POST" else None, token=at)
    ok(s in (404, 405), f"{method} {path} is gone ({s})")
s, d = call("GET", f"/v1/partner/restaurants/{pho['id']}/dashboard", token=at)
ok(s == 200 and "liveOrders" not in d and "openRequests" not in d and "arrivedToday" in d and "pendingCounterPayments" in d, "dashboard shows arrivals and counter payments, no orders")
ok("digitalMenuEnabled" not in pho["settings"] and "autoProgressOrders" not in pho["settings"] and "hasDigitalMenu" not in pho, "no table-ordering settings")
s, m = call("GET", "/v1/admin/metrics", token=at)
ok(s == 200 and "liveSessions" not in m, "admin metrics have no table sessions")

# 3. Walk-in: no Feast booking, no paying on Feast
walk_tok, walk = diner(f"+614{uuid.uuid4().int % 10**8:08d}", "Wally Walkin")
s, e = call("POST", "/v1/bills/quote", {"restaurantId": pho["id"], "billAmount": 8000}, token=walk_tok)
ok(s == 409 and e.get("code") == "BOOKING_REQUIRED", "a walk-in can't pay on Feast (book first)")
s, e = call("POST", "/v1/bills/quote", {"restaurantId": pho["id"], "billAmount": 8000, "sessionId": "ts_old"}, token=walk_tok)
ok(s == 409 and e.get("code") == "BOOKING_REQUIRED", "…not even by naming an old table session")

# 4. Booked diner: find a slot within the check-in window (30 min before the slot) at the server's "now"
syd = NOW.astimezone(ZoneInfo("Australia/Sydney"))
today = syd.strftime("%Y-%m-%d")
s, avail = call("GET", f"/v1/restaurants/{pho['id']}/slots?date={today}&guests=2")
slot = later = None
for sl in avail.get("slots", []) if s == 200 else []:
    at_local = datetime.strptime(f"{today} {sl['time']}", "%Y-%m-%d %H:%M").replace(tzinfo=ZoneInfo("Australia/Sydney"))
    # The server's clock has moved on a little since NOW; allow for it.
    if sl["available"] and slot is None and timedelta(0) < at_local - syd <= timedelta(minutes=30):
        slot = sl
    elif sl["available"] and later is None and at_local - syd > timedelta(minutes=60):
        later = sl

booked_tok, booked = diner(f"+614{uuid.uuid4().int % 10**8:08d}", "Bea Booked")


def book(sl):
    return call("POST", "/v1/bookings", {
        "restaurantId": pho["id"], "date": today, "slotId": sl["id"], "guests": 2,
        "contact": {"name": "Bea Booked", "phone": booked["phone"]}, "idempotencyKey": uuid.uuid4().hex,
    }, token=booked_tok)


if later is not None:
    s, res = book(later)
    if s == 201:
        s, e = call("POST", f"/v1/bookings/{res['booking']['id']}/check-in", token=booked_tok)
        ok(s == 409 and e.get("code") == "CHECK_IN_UNAVAILABLE", f"check-in isn't open an hour before the slot ({s} {e.get('code')})")
        call("POST", f"/v1/bookings/{res['booking']['id']}/cancel", token=booked_tok)

if slot is None:
    skip(f"no pho-republic slot bookable and within 30 min of {syd:%H:%M} Sydney — run the server under faketime (see the docstring)")
else:
    s, res = book(slot)
    ok(s == 201 and res["booking"]["status"] == "CONFIRMED", f"booked {slot['time']} ({s})")
    bk = res["booking"]
    # "I'm here"
    s, e = call("POST", f"/v1/bookings/{bk['id']}/check-in", token=walk_tok)
    ok(s == 404, "someone else can't check in a booking")
    s, c1 = call("POST", f"/v1/bookings/{bk['id']}/check-in", token=booked_tok)
    ok(s == 200 and c1.get("checkedInAt"), f"diner checks in ({s})")
    s, c2 = call("POST", f"/v1/bookings/{bk['id']}/check-in", token=booked_tok)
    ok(s == 200 and c2.get("checkedInAt") == c1.get("checkedInAt"), "checking in twice changes nothing")
    s, bookings = call("GET", f"/v1/partner/restaurants/{pho['id']}/bookings?date={today}", token=at)
    mine = [b for b in (bookings if isinstance(bookings, list) else bookings.get("bookings", [])) if b["id"] == bk["id"]]
    ok(s == 200 and mine and mine[0].get("checkedInAt"), "restaurant sees the diner has arrived")
    s, d = call("GET", f"/v1/partner/restaurants/{pho['id']}/dashboard", token=at)
    ok(s == 200 and d["arrivedToday"] >= 1, f"dashboard counts arrivals ({d.get('arrivedToday')})")
    # Pay through the QR path: scan → restaurant → quote with the bill total → pay.
    s, r = call("GET", f"/v1/qr/{code}", token=booked_tok)
    s, bill = call("POST", "/v1/bills/quote", {"restaurantId": r["restaurant"]["id"], "billAmount": 8000}, token=booked_tok)
    ok(s == 200 and bill.get("bookingId") == bk["id"] and bill["billAmount"] == 8000 and "sessionId" not in bill and "source" not in bill,
       f"the diner's bill is priced against their booking ({s})")
    ok(bill["savings"] > 0 and bill["payable"] < bill["billAmount"], f"…with the Feast deal ({bill.get('savings')} off)")
    s, i = call("POST", "/v1/payments/intents", {"purpose": "BILL", "referenceId": bill["id"]}, token=booked_tok)
    ok(s == 201, f"payment started ({s})")
    if s == 201:
        s, paid = call("POST", f"/v1/payments/intents/{i['id']}/confirm", {"method": "CARD"}, token=booked_tok)
        ok(s == 200 and paid["status"] == "SUCCEEDED", "the diner pays in the app")

print("FAILURES:", fails, "SKIPPED:", skips)
sys.exit(1 if fails else 0)
