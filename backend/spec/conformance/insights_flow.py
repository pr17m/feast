"""HTTP conformance test for the insights screens (owner scorecard, diner
savings, admin marketplace health) against any Feast backend serving
Australia with development seed data (owner.au@feast.demo, diner +61412345678,
admin@feast.demo):

    python3 insights_flow.py http://localhost:8080      # exit code 1 on any failure

Covers: who may read which insights, period validation, the shape of each
response, and that the numbers agree with the underlying lists (bill payments,
restaurants). Read-only apart from signing in a fresh diner.
"""
import json, os, random, sys, urllib.request

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
rid = mine[0]["id"]
mine_ids = {r["id"] for r in mine}
s, all_r = call("GET", "/v1/admin/restaurants", token=admin)
other = next(r["id"] for r in all_r if r["id"] not in mine_ids)
print(f"owner restaurant {rid}, someone else's {other}")

# ---------------------------------------------------------------- owner
s, o = call("GET", f"/v1/partner/restaurants/{rid}/insights?days=30", token=owner)
ok(s == 200, f"owner reads own insights ({s})")
ok(o["restaurant"]["id"] == rid and o["period"]["days"] == 30 and o["period"].get("from") and o["period"]["to"], "owner: restaurant and 30-day period")
ok(o["currency"] == "AUD", "owner: money in the restaurant's currency")
f = o["bookings"]
ok(f["honoured"] + f["noShow"] <= f["booked"], "owner: kept + no-shows ≤ booked")
ok(o["newDiners"] + o["repeatDiners"] == o["feastDiners"], "owner: new + returning = Feast diners")
ok(len(o["byWeekday"]) == 7 and [w["day"] for w in o["byWeekday"]][0] == "Mon", "owner: seven weekdays, Monday first")
ok(sum(w["revenue"] for w in o["byWeekday"]) == o["revenue"], "owner: weekday revenue adds up to revenue")
ok(len(o["ratingTrend"]) == 6, "owner: six months of rating trend")
ok(all(0 <= h["hour"] <= 23 for h in o["byHour"]), "owner: hours are 0–23")
if o["feastCost"] > 0:
    ok(abs(o["returnOnCost"] - o["revenue"] / o["feastCost"]) < 1e-6, "owner: return on cost = revenue ÷ cost")
else:
    ok("returnOnCost" not in o, "owner: no return on cost without a cost")
if o["averageBill"] is not None and o["billsPaid"]:
    ok(abs(o["averageBill"] - o["revenue"] / o["billsPaid"]) <= 1, "owner: average bill")
ok(o["deals"]["dealsUsed"] <= o["billsPaid"], "owner: discounted bills ≤ bills paid")
s7, o7 = call("GET", f"/v1/partner/restaurants/{rid}/insights?days=7", token=owner)
s90, o90 = call("GET", f"/v1/partner/restaurants/{rid}/insights?days=90", token=owner)
ok(s7 == 200 and s90 == 200 and o7["billsPaid"] <= o["billsPaid"] <= o90["billsPaid"], "owner: longer periods include shorter ones")
s, e = call("GET", f"/v1/partner/restaurants/{rid}/insights?days=45", token=owner)
ok(s == 400 and e.get("code") == "INVALID_PERIOD", f"owner: unsupported period refused ({s})")
s, _ = call("GET", f"/v1/partner/restaurants/{other}/insights", token=owner)
ok(s in (403, 404), f"owner can't read another restaurant's insights ({s})")
s, _ = call("GET", f"/v1/partner/restaurants/{rid}/insights", token=diner)
ok(s == 403, f"diner can't read a restaurant's insights ({s})")
s, _ = call("GET", f"/v1/partner/restaurants/{rid}/insights")
ok(s == 401, f"signed-out caller refused ({s})")
s, a_o = call("GET", f"/v1/partner/restaurants/{rid}/insights?days=30", token=admin)
ok(s == 200 and a_o["revenue"] == o["revenue"], "admin can read a restaurant's insights (same numbers)")

# ---------------------------------------------------------------- diner
s, d = call("GET", "/v1/me/insights", token=diner)
ok(s == 200 and d["period"]["days"] == 0 and "from" not in d["period"], f"diner reads all-time insights ({s})")
s, pays = call("GET", "/v1/me/bill-payments", token=diner)
ok(d["billsPaid"] == len(pays), f"diner: bills paid matches their receipts ({d['billsPaid']} vs {len(pays)})")
ok(d["totalSaved"] == sum(p["bill"]["savings"] for p in pays), "diner: total saved = savings on their receipts")
ok(d["totalSpent"] >= sum(p["payment"]["amount"] for p in pays), "diner: total paid includes every bill")
ok(len(d["favouriteRestaurants"]) <= 3 and len(d["favouriteCuisines"]) <= 3, "diner: top 3 favourites and cuisines")
ok(all(f["restaurant"]["id"] for f in d["favouriteRestaurants"]), "diner: favourites name their restaurant")
s30, d30 = call("GET", "/v1/me/insights?days=30", token=diner)
ok(s30 == 200 and d30["period"]["days"] == 30 and d30["billsPaid"] <= d["billsPaid"], "diner: 30-day view is a subset")
s, e = call("GET", "/v1/me/insights?days=7", token=diner)
ok(s == 400, f"diner: unsupported period refused ({s})")
fresh = otp(f"+614129{random.randint(10000, 99999)}")
s, dn = call("GET", "/v1/me/insights", token=fresh)
ok(s == 200 and dn["visits"] == 0 and dn["totalSaved"] == 0 and dn["billsPaid"] == 0, "a new diner sees only their own (empty) history")
s, _ = call("GET", "/v1/me/insights", token=owner)
ok(s == 403, f"owners have no diner insights ({s})")
s, _ = call("GET", "/v1/me/insights")
ok(s == 401, f"signed-out caller refused ({s})")

# ---------------------------------------------------------------- admin
s, a = call("GET", "/v1/admin/insights?days=30", token=admin)
ok(s == 200 and a["period"]["days"] == 30, f"admin reads marketplace insights ({s})")
ok(a["restaurants"]["live"] == sum(1 for r in all_r if r["status"] == "LIVE"), "admin: live restaurants match the restaurant list")
ok(a["restaurants"]["pendingApproval"] == sum(1 for r in all_r if r["status"] == "PENDING_APPROVAL"), "admin: pending approvals match")
ok(any(m["currency"] == "AUD" for m in a["money"]), "admin: AUD money")
ok(len({m["currency"] for m in a["money"]}) == len(a["money"]), "admin: one row per currency (never summed across)")
ok(sum(c["liveRestaurants"] for c in a["cities"]) == a["restaurants"]["live"], "admin: live restaurants by city add up")
ok(len(a["topRestaurants"]) <= 5, "admin: top five restaurants")
ok(a["disputes"]["openNow"] >= a["disputes"]["withFeastNow"], "admin: cases with Feast are among the open ones")
ok(a["bookings"]["honoured"] + a["bookings"]["noShow"] <= a["bookings"]["booked"], "admin: kept + no-shows ≤ booked")
s7, a7 = call("GET", "/v1/admin/insights?days=7", token=admin)
ok(s7 == 200 and sum(m["billsPaid"] for m in a7["money"]) <= sum(m["billsPaid"] for m in a["money"]), "admin: 7 days ⊆ 30 days")
s, _ = call("GET", "/v1/admin/insights?days=1", token=admin)
ok(s == 400, f"admin: unsupported period refused ({s})")
s, _ = call("GET", "/v1/admin/insights", token=owner)
ok(s == 403, f"owner can't read marketplace insights ({s})")
s, _ = call("GET", "/v1/admin/insights", token=diner)
ok(s == 403, f"diner can't read marketplace insights ({s})")

print(f"FAILURES: {fails}")
sys.exit(1 if fails else 0)
