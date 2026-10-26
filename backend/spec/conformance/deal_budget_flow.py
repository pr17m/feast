"""HTTP conformance test for the restaurant deal budget (per-bill cap and daily
deal limit) against any Feast backend serving Australia with development seed
data (owner.au@feast.demo). Feast deals are for diners who booked on Feast
today (one discounted bill per booking), so each discounted bill here is from a
new diner with a booking; that needs a slot still open today (late at night,
run the server under libfaketime — see booking_deals_flow.py):

    python3 deal_budget_flow.py http://localhost:8080      # exit code 1 on any failure

Covers: validation, the per-bill cap on the Feast deal, the daily limit (bills
stay payable without the deal once it's used up), holds during payment (a
second payment can't take the last deal; a failed payment gives it back),
who can read and change the budget. Leaves the restaurant's limits cleared.
"""
import json, os, sys, urllib.request, uuid
BASE = sys.argv[1].rstrip("/")
PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")


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
        return e.code, json.loads(e.read() or b"{}")


fails = 0


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


owner, admin, diner = login("owner.au@feast.demo"), login("admin@feast.demo"), otp("+61412345678")
s, mine = call("GET", "/v1/partner/restaurants", token=owner)
# A live restaurant taking bill payments with a percentage bill offer.
r = next(x for x in mine if x["status"] == "LIVE" and x["settings"]["billPaymentEnabled"] and any(o["scope"] == "BILL" and o.get("percentOff") for o in x["offers"]))
rid = r["id"]
print(f"using {r['name']} ({rid})")
budget_path = f"/v1/partner/restaurants/{rid}/deal-budget"
settings_path = f"/v1/partner/restaurants/{rid}/settings"


def budget():
    s, b = call("GET", budget_path, token=owner)
    assert s == 200, f"budget {s} {b}"
    return b


def booked_diner():
    """A new diner with a Feast booking at the restaurant today."""
    phone = f"+614{uuid.uuid4().int % 10**8:08d}"
    tok = otp(phone)
    today = budget()["date"]
    s, avail = call("GET", f"/v1/restaurants/{rid}/slots?date={today}&guests=2")
    slot = next((sl for sl in (avail.get("slots", []) if s == 200 else []) if sl["available"]), None)
    if not slot:
        print(f"SKIP no slot left today at {r['name']} — run the server under faketime (see booking_deals_flow.py)")
        sys.exit(0)
    s, res = call("POST", "/v1/bookings", {"restaurantId": rid, "date": today, "slotId": slot["id"], "guests": 2,
                                          "contact": {"name": "Deal Tester", "phone": phone}, "idempotencyKey": uuid.uuid4().hex}, token=tok)
    assert s == 201, f"book {s} {res}"
    if res["booking"]["status"] == "PENDING_PAYMENT":
        s, pi = call("POST", "/v1/payments/intents", {"purpose": "BOOKING", "referenceId": res["booking"]["id"]}, token=tok)
        call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD"}, token=tok)
    return tok


# Who quoted each bill (a bill is paid by the diner who quoted it).
payer = {}


def quote(amount=20000):
    tok = booked_diner()
    s, bill = call("POST", "/v1/bills/quote", {"restaurantId": rid, "billAmount": amount, "tip": 0}, token=tok)
    assert s in (200, 201), f"quote {s} {bill}"
    payer[bill["id"]] = tok
    return bill


def intent(bill):
    return call("POST", "/v1/payments/intents", {"purpose": "BILL", "referenceId": bill["id"]}, token=payer[bill["id"]])


def pay(bill):
    s, pi = intent(bill)
    assert s == 201, f"intent {s} {pi}"
    s, paid = call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD"}, token=payer[bill["id"]])
    return paid


# ------------------------------------------------------------ access + validation
b0 = budget()
ok("used" in b0 and len(b0["date"]) == 10, f"owner reads today's budget ({b0['used']} used on {b0['date']})")
s, _ = call("GET", budget_path, token=admin)
ok(s == 200, "admin can view the budget")
s, _ = call("GET", budget_path, token=diner)
ok(s in (403, 404), f"diners can't ({s})")
s, e = call("PATCH", settings_path, {"dailyDealLimit": 0}, token=owner)
ok(s == 400 and e.get("code") == "INVALID_SETTINGS", "0 deals per day refused (clear it with null instead)")
s, e = call("PATCH", settings_path, {"maxDiscountPerBill": 50}, token=owner)
ok(s == 400, "a cap below one dollar is refused")
s, e = call("PATCH", settings_path, {"maxDiscountPerBill": 10050}, token=owner)
ok(s == 400, "a cap in cents is refused (whole dollars only)")

# ------------------------------------------------------------ per-bill cap
s, st = call("PATCH", settings_path, {"maxDiscountPerBill": 500, "dailyDealLimit": None}, token=owner)
ok(s == 200 and st.get("maxDiscountPerBill") == 500 and "dailyDealLimit" not in st, f"owner sets a A$5 cap ({st})")
s, pub = call("GET", f"/v1/restaurants/{r['slug']}")
ok(s == 200, "public page still loads")
bill = quote(20000)
ok(0 < bill["savings"] <= 500, f"deal capped at A$5 (saves {bill['savings']})")
ok("caps Feast deals" in (bill.get("dealNotice") or ""), f"diner told why: {bill.get('dealNotice')}")
ok(bill["payable"] == bill["billAmount"] - bill["savings"], "payable = bill − capped deal")

# ------------------------------------------------------------ daily limit
used = budget()["used"]
s, st = call("PATCH", settings_path, {"maxDiscountPerBill": None, "dailyDealLimit": used + 2}, token=owner)
ok(s == 200 and st.get("dailyDealLimit") == used + 2 and "maxDiscountPerBill" not in st, f"limit set to {used + 2}, cap cleared")
p1 = pay(quote(20000))
ok(p1["status"] == "SUCCEEDED", "1st discounted bill paid")
b = budget()
ok(b["used"] == used + 1 and b["remaining"] == 1, f"budget shows 1 left ({b['used']}/{b['dailyDealLimit']})")

# Two diners race for the last deal: the first payment holds it.
first, second = quote(20000), quote(30000)
ok(first["savings"] > 0 and second["savings"] > 0, "both quotes still show the deal")
s, pi1 = intent(first)
ok(s == 201, "1st payment holds the last deal")
s, e = intent(second)
ok(s == 409 and e.get("code") == "DEALS_USED_UP", f"2nd payment refused while it's held ({s} {e.get('code')})")
# The held payment fails: the deal goes back.
s, failed = call("POST", f"/v1/payments/intents/{pi1['id']}/confirm", {"method": "CARD", "simulate": "failure"}, token=payer[first["id"]])
ok(failed["status"] == "FAILED", "1st payment declined")
ok(budget()["remaining"] == 1, "declined payment gave the deal back")
paid2 = pay(second)
ok(paid2["status"] == "SUCCEEDED", "2nd diner gets the last deal")
b = budget()
ok(b["used"] == used + 2 and b["remaining"] == 0, f"all {b['dailyDealLimit']} deals used")
# Retrying the declined bill: it was priced with the deal, which is now gone.
s, e = intent(first)
ok(s == 409 and e.get("code") == "DEALS_USED_UP", "the stale discounted bill can't be paid")
late = quote(20000)
ok(late["savings"] == 0 and late["payable"] == late["billAmount"], "new bills are payable without the deal")
ok("used up" in (late.get("dealNotice") or ""), f"diner sees why: {late.get('dealNotice')}")
p3 = pay(late)
ok(p3["status"] == "SUCCEEDED", "full-price bill still paid on Feast")
ok(budget()["used"] == used + 2, "full-price bills don't use the budget")

# ------------------------------------------------------------ owner raises it again
s, st = call("PATCH", settings_path, {"dailyDealLimit": None}, token=owner)
ok(s == 200 and "dailyDealLimit" not in st, "owner removes the limit")
ok(quote(20000)["savings"] > 0, "deals are back")
b = budget()
ok(b.get("remaining") is None and b["used"] == used + 2, "unlimited, count kept")

print("FAILURES:", fails)
sys.exit(1 if fails else 0)
