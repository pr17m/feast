# Deploying Feast

Feast runs **one independent stack per country**. India, Australia, and later
New Zealand, Canada and the UK each have their own database, Redis, file
storage and URL. Nothing is shared between countries. Every country runs the
same code and the same container images. The country is chosen when you deploy
(`FEAST_COUNTRY=au`), which loads `backend/config/countries/au.yaml`
(currency, tax, payment provider, licence documents, cities).

```
             ┌──────────────────────── one country ───────────────────────┐
 browser ──▶ │ web (Next.js :3000) ──/v1/*──▶ api (Rust :8080) ──▶ Postgres │
             │   only public port              │  ▲                 Redis    │
             │                                 └──┘ worker          files    │
             └─────────────────────────────────────────────────────────────┘
```

## Pick a target

| Where                                          | Folder                | Command                    | Good for                         |
|------------------------------------------------|-----------------------|----------------------------|----------------------------------|
| Your laptop / any VM (Docker)                  | `compose/`            | `docker compose … up -d`   | trying it, demos, small launches |
| **AWS** (ECS Fargate, RDS, ElastiCache, S3)    | `terraform/aws/`      | `./deploy.sh au`           | production                       |
| **Azure** (Container Apps, PostgreSQL Flexible, Azure Cache for Redis, Blob) | `terraform/azure/` | `./deploy.sh au` | production |
| **Google Cloud** (Cloud Run, Cloud SQL, Memorystore, Cloud Storage) | `terraform/gcp/` | `./deploy.sh au` | production |

Each cloud script does everything:
1. creates the image registry
2. builds and pushes the images (Azure and GCP build in the cloud, so you don't need local Docker)
3. creates the database, Redis, storage and app, and prints the URL

Running it again deploys a new version.

## Cloud quick start

You need [Terraform](https://developer.hashicorp.com/terraform/install) ≥ 1.6
plus that cloud's CLI, logged in.

```sh
# AWS — needs Docker (builds ARM64 images locally; native on Apple Silicon)
aws configure                      # or aws sso login
cd deploy/terraform/aws && ./deploy.sh au

# Azure — no local Docker needed
az login && az account set -s <subscription-id>
cd deploy/terraform/azure && ./deploy.sh au

# Google Cloud — no local Docker needed; put your project id in au.tfvars first
gcloud auth login && gcloud auth application-default login
cd deploy/terraform/gcp && ./deploy.sh au
```

The first run takes 15–30 minutes, because the managed database and Redis are
slow to create. After that, `./deploy.sh au` ships a new version in a few
minutes.

The demo admin/owner password is generated per stack:

```sh
terraform output -raw seed_password   # admin@feast.demo, owner.au@feast.demo …
```

India is the same with `./deploy.sh in`. Each country gets its own
**Terraform workspace**, so state never mixes.

> **Team use:** store Terraform state remotely before sharing a stack:
> - AWS: an S3 backend
> - Azure: an `azurerm` storage backend
> - GCP: a `gcs` backend
>
> Add a `backend` block in `versions.tf`.

## Development vs production

`environment` in the `.tfvars` file (or `FEAST_ENV` with Docker):

|                     | `development` (default)                         | `production`                                                               |
|---------------------|-------------------------------------------------|----------------------------------------------------------------------------|
| Diner sign-in (OTP) | code `123456`, no SMS                           | real SMS via **Twilio Verify** (required)                                  |
| Payments            | test mode, no money moves                       | **Razorpay** (India) / **Stripe** (Australia…) with your keys              |
| Social sign-in      | test mode (type any email)                      | off unless you add real client IDs                                         |
| Demo data           | loaded into an empty database                   | not loaded                                                                 |
| Safety checks       | warnings                                        | refuses to start with unsafe settings, and lists what to fix               |

To go live, add to `au.tfvars` (Australia) or `in.tfvars` (India):

```hcl
environment            = "production"
stripe_publishable_key = "pk_live_…"          # Australia: Stripe
stripe_secret_key      = "sk_live_…"
# razorpay_key_id      = "rzp_live_…"         # India: Razorpay
# razorpay_key_secret  = "…"
twilio_account_sid     = "AC…"                # OTP SMS (both countries)
twilio_auth_token      = "…"
twilio_service_sid     = "VA…"
extra_env = {                                 # anything else, e.g. Google sign-in
  FEAST__AUTH__OAUTH__GOOGLE__MODE       = "live"
  FEAST__AUTH__OAUTH__GOOGLE__CLIENT_IDS = "1234-abc.apps.googleusercontent.com"
}
```

**Payment webhooks** (recommended). The server already verifies each payment
when the customer confirms; webhooks are a second, independent signal from the
provider, so a payment still settles if the customer closes the tab mid-way.
In the provider dashboard add the endpoint and copy its signing secret:

| Provider | Endpoint | Events | Setting |
|---|---|---|---|
| Stripe | `https://<your-site>/v1/webhooks/payments/stripe` | `payment_intent.succeeded`, `payment_intent.payment_failed` | `stripe_webhook_secret = "whsec_…"` |
| Razorpay | `https://<your-site>/v1/webhooks/payments/razorpay` | `payment.captured`, `payment.failed`, `order.paid` | `razorpay_webhook_secret = "…"` |

Signatures are checked (Stripe also rejects events older than 5 minutes), the
amount and currency must match the payment, and a payment is applied exactly
once even if the webhook and the customer's confirmation arrive together.

Keep secrets out of git: use a `*.auto.tfvars` file that isn't committed, or
`TF_VAR_stripe_secret_key=… ./deploy.sh au`.

Also for real data:

- **Your domain.** Each cloud gives you an HTTPS URL right away. To use your own domain:
  - AWS: set `domain_name` + `certificate_arn`.
  - Azure: add a custom domain to the Container App.
  - GCP: map a domain to Cloud Run.
- **Data protection.** `protect_data = true` turns on database deletion protection and final snapshots, and keeps the document storage on destroy.
- **Scale.** Each instance runs the web app plus the API (gateway + worker).
  - AWS: `desired_count`
  - Azure: `min_replicas` / `max_replicas`
  - GCP: `min_instances` / `max_instances`

## Phone apps

Before submitting the iOS and Android apps, work through
[`STORE_RELEASE_CHECKLIST.md`](STORE_RELEASE_CHECKLIST.md): push notifications
(`fcm_credentials_json`), reviewer sign-in (`review_numbers`, `review_code`),
app links (`ios_app_ids`, `android_apps`) and the account-deletion URL.

## Adding a country

1. Copy `backend/config/countries/au.yaml` to e.g. `nz.yaml`. Set the currency, tax, payment provider, phone rules, licence documents and cities.
2. Add the country's public URL to `backend/config/directory.toml`, so other sites can send visitors there.
3. Deploy it: create `nz.tfvars` (country + region) and run `./deploy.sh nz`.

You don't change any code. To add a new payment provider for a country, see
`backend/rust/crates/infra/src/payments/mod.rs`: one adapter file plus one line.

## What runs where

| Part           | Docker (`compose/`)    | AWS                    | Azure                        | GCP                          |
|----------------|------------------------|------------------------|------------------------------|------------------------------|
| web + API      | containers             | ECS Fargate task (2 containers) | Container App (2 containers) | Cloud Run (2 containers) |
| PostgreSQL 16  | `postgres` container   | RDS                    | Flexible Server              | Cloud SQL                    |
| Redis          | `redis` container      | ElastiCache            | Azure Cache for Redis (TLS)  | Memorystore                  |
| Documents      | Docker volume          | S3                     | Blob Storage                 | Cloud Storage                |
| Secrets        | `.env` file            | SSM Parameter Store    | Container App secrets        | Secret Manager               |
| Logs           | `docker compose logs`  | CloudWatch             | Log Analytics                | Cloud Logging                |
| Images         | built locally          | ECR                    | ACR (`az acr build`)         | Artifact Registry (Cloud Build) |

The API runs database migrations on start. They are safe to run from many
instances at once (Postgres advisory lock).

Health checks:

- web: `/healthz`
- API: `/healthz`, `/readyz` (checks the database, bus, notifications and storage)
- Prometheus metrics: `/metrics` on the API. It is private: set `FEAST__HTTP__METRICS_TOKEN`.
