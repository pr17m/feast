# Run Feast for one country with Docker

This runs the whole stack for one country on one machine: your Mac, or any VM
on AWS, Azure or GCP. It starts:

| Service    | What it is                                                       |
|------------|------------------------------------------------------------------|
| `postgres` | the country's database (*optional*: or use a Postgres you already run) |
| `redis`    | message bus between the API and the worker                       |
| `migrate`  | applies database migrations, then exits                          |
| `api`      | the Rust API (+ live updates); loads demo data on first start    |
| `worker`   | background jobs: notifications, expiry reminders                 |
| `web`      | the Next.js web app — the only public port; forwards `/v1/*` to the API |
| `tunnel`   | *optional* public `https://…trycloudflare.com` URL to share it   |

## 1. Install Docker

Install **Docker Desktop** (Mac/Windows) or Docker Engine (Linux), then check:

```sh
docker compose version
```

## 2. Configure the country

```sh
cd feast/deploy/compose
cp au.env.example au.env
```

Open `au.env` and change the passwords:

- `POSTGRES_PASSWORD`: for the bundled database (see below)
- `JWT_SECRET`: 32+ random characters
- `SEED_PASSWORD`: the password for the demo owner/admin accounts

You can generate random values with `openssl rand -hex 24`.

**Database.** By default (`COMPOSE_PROFILES=db`) the stack runs its own
Postgres. If you already run Postgres on this machine, delete the
`COMPOSE_PROFILES=db` line and point at yours instead:

```sh
DATABASE_URL=postgres://postgres:postgres@host.docker.internal:5432/feast
```

`host.docker.internal` means "this computer" from inside Docker. Use a
separate database per country (e.g. `feast` for Australia, `feast_in` for India:
`docker exec -it postgres psql -U postgres -c "create database feast_in"`).

## 3. Start it

```sh
docker compose --env-file au.env up -d --build
```

The first build takes about 10 minutes (it compiles the Rust API). After that,
starts take seconds. Open **http://localhost:8081**.

Check that it's healthy:

```sh
docker compose --env-file au.env ps
curl -s localhost:8081/v1/markets   # API answers through the web app
```

## 4. Share it (e.g. with someone in Australia)

```sh
docker compose --env-file au.env --profile share up -d
./share-url.sh au.env        # → https://something-random.trycloudflare.com
```

Send that link. It works from anywhere, over HTTPS, as long as your machine is
on and Docker is running.

- The link changes each time the tunnel restarts.
- For a permanent address, use a named Cloudflare tunnel or deploy to a cloud
  (see `../README.md`).

### Signing in (development mode)

| Who            | How                                                                                                  |
|----------------|------------------------------------------------------------------------------------------------------|
| Diner          | any Australian mobile (e.g. `412 345 678`). The code is **123456** (no SMS is sent in development mode). |
| Diner (social) | "Continue with Google/Apple/Facebook" runs in test mode: type any name and email.                    |
| Owner          | `owner.au@feast.demo` with your `SEED_PASSWORD`                                                      |
| Admin          | `admin@feast.demo` with your `SEED_PASSWORD`                                                         |
| New restaurant | "List it on Feast" → sign up → upload documents → admin approves                                     |

Payments run in **test mode**: no money moves, and you can simulate a decline.

## Real SMS and payments (going live)

Set these in `au.env` (Australia uses Stripe, India uses Razorpay):

```sh
FEAST_ENV=production
PAYMENTS_FORCE_PROVIDER=
STRIPE_ENABLED=true
STRIPE_PUBLISHABLE_KEY=pk_live_...
STRIPE_SECRET_KEY=sk_live_...
```

For real OTP SMS, also add these to the `api`/`worker` environment (Twilio Verify):

- `FEAST__AUTH__OTP__PROVIDER=twilio`
- `FEAST__AUTH__OTP__TWILIO__ACCOUNT_SID`
- `FEAST__AUTH__OTP__TWILIO__AUTH_TOKEN`
- `FEAST__AUTH__OTP__TWILIO__SERVICE_SID`

Payment webhooks (recommended once payments are real): in the Stripe dashboard
add the endpoint `https://<your-site>/v1/webhooks/payments/stripe` with events
`payment_intent.succeeded` and `payment_intent.payment_failed`, then set
`STRIPE_WEBHOOK_SECRET=whsec_...` (Razorpay: `/v1/webhooks/payments/razorpay`,
events `payment.captured`, `payment.failed`, `order.paid`, `RAZORPAY_WEBHOOK_SECRET`).

Production mode refuses to start with unsafe settings, and lists what to fix.

## Dish photos

Owners upload a photo per dish in Partner → Menu (JPG, PNG or WebP, up to 5 MB).
Photos are stored with the other files (the `files` volume, or S3/Azure) under
`media/` and served publicly at `/v1/media/…`. For dishes without a photo, Feast
can add a matching stock photo from Unsplash a few at a time (owners can remove it
or press "Find a photo"). Create a free app at https://unsplash.com/developers and set:

```sh
STOCK_PHOTOS_PROVIDER=unsplash
UNSPLASH_ACCESS_KEY=...
```

Without a key, dishes without a photo show generated cuisine art.

## Everyday commands

```sh
docker compose --env-file au.env logs -f api worker      # logs
docker compose --env-file au.env down                    # stop (keeps data)
docker compose --env-file au.env down -v                 # stop and DELETE all data (bundled database only)
docker compose --env-file au.env up -d --build           # update after code changes
```

After code changes, `./redeploy.sh` does all of it in one go: rebuilds the images,
restarts the stack (data is kept, migrations run first), waits until Feast answers
and removes old images. `./redeploy.sh in` for India, `--photos` to refresh the demo
restaurants' photos, `--no-cache` for a clean rebuild.

## Policy pages

Terms, privacy, refunds and contact pages live at `/legal/...` and are linked from
every page's footer and the sign-in screens. Fill in the `FEAST_LEGAL_*` and
`FEAST_SUPPORT_*` lines in your env file (see the example file). Until a lawyer has
reviewed the text and you set `FEAST_LEGAL_REVIEWED=true`, the pages show a
"draft" notice.

## India too

```sh
cp in.env.example in.env
docker compose --env-file in.env up -d --build
```

This opens on http://localhost:8082. It's a completely separate stack, with
its own database and data.
