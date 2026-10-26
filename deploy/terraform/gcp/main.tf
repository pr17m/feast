# Feast on Google Cloud — one country per stack (Terraform workspace per country).
#
#   Internet → Cloud Run service (HTTPS) → [web :3000 → api :8080 sidecar (localhost)]
#   api → Cloud SQL PostgreSQL (unix socket), Memorystore Redis (VPC), Cloud Storage (S3 interop)
#
# Images live in this stack's Artifact Registry (built with Cloud Build).

locals {
  name = "feast-${var.country}"
  prod = var.environment == "production"
  apis = ["run.googleapis.com", "sqladmin.googleapis.com", "redis.googleapis.com", "artifactregistry.googleapis.com", "cloudbuild.googleapis.com", "secretmanager.googleapis.com", "compute.googleapis.com", "iam.googleapis.com"]
}

resource "google_project_service" "apis" {
  for_each           = toset(local.apis)
  service            = each.value
  disable_on_destroy = false
}

resource "random_password" "db" {
  length  = 32
  special = false
}

resource "random_password" "jwt" {
  length  = 48
  special = false
}

resource "random_password" "seed" {
  length  = 20
  special = false
}

# ------------------------------------------------------------------ images

resource "google_artifact_registry_repository" "images" {
  repository_id = local.name
  location      = var.region
  format        = "DOCKER"
  depends_on    = [google_project_service.apis]
}

# ------------------------------------------------------------------ network (for Memorystore)

resource "google_compute_network" "main" {
  name                    = local.name
  auto_create_subnetworks = false
  depends_on              = [google_project_service.apis]
}

resource "google_compute_subnetwork" "main" {
  name          = local.name
  network       = google_compute_network.main.id
  region        = var.region
  ip_cidr_range = "10.50.0.0/24"
}

# ------------------------------------------------------------------ data

resource "google_sql_database_instance" "main" {
  name                = local.name
  database_version    = "POSTGRES_16"
  region              = var.region
  deletion_protection = var.protect_data
  settings {
    tier              = var.sql_tier
    edition           = "ENTERPRISE"
    availability_type = "ZONAL"
    ip_configuration {
      ipv4_enabled = true # reached only through the Cloud SQL connector (IAM-authorised), no authorised networks
    }
    backup_configuration {
      enabled                        = true
      point_in_time_recovery_enabled = true
    }
  }
  depends_on = [google_project_service.apis]
}

resource "google_sql_database" "feast" {
  name     = "feast"
  instance = google_sql_database_instance.main.name
}

resource "google_sql_user" "feast" {
  name     = "feast"
  instance = google_sql_database_instance.main.name
  password = random_password.db.result
}

resource "google_redis_instance" "main" {
  name               = local.name
  tier               = "BASIC"
  memory_size_gb     = 1
  region             = var.region
  redis_version      = "REDIS_7_2"
  authorized_network = google_compute_network.main.id
  depends_on         = [google_project_service.apis]
}

resource "google_storage_bucket" "documents" {
  name                        = "${local.name}-documents-${var.project_id}"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = !var.protect_data
}

# ------------------------------------------------------------------ identity

resource "google_service_account" "app" {
  account_id   = "${local.name}-app"
  display_name = "Feast ${upper(var.country)} app"
}

resource "google_project_iam_member" "sql" {
  project = var.project_id
  role    = "roles/cloudsql.client"
  member  = "serviceAccount:${google_service_account.app.email}"
}

resource "google_storage_bucket_iam_member" "documents" {
  bucket = google_storage_bucket.documents.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.app.email}"
}

# The API uses the S3-compatible XML API with an HMAC key of the service account.
resource "google_storage_hmac_key" "app" {
  service_account_email = google_service_account.app.email
}

# ------------------------------------------------------------------ secrets

locals {
  db_url = "postgres://feast:${random_password.db.result}@localhost/feast?host=/cloudsql/${google_sql_database_instance.main.connection_name}"
  secrets = {
    "database-url"    = { env = ["FEAST__DATABASE__URL", "FEAST__NOTIFICATIONS__DATABASE_URL"], value = local.db_url }
    "jwt-secret"      = { env = ["FEAST__AUTH__JWT_SECRET"], value = random_password.jwt.result }
    "seed-password"   = { env = ["FEAST__APP__SEED_PASSWORD", "FEAST__APP__INITIAL_PARTNER_PASSWORD"], value = random_password.seed.result }
    "storage-secret"  = { env = ["FEAST__STORAGE__S3__SECRET_ACCESS_KEY"], value = google_storage_hmac_key.app.secret }
    "stripe-secret"   = { env = ["FEAST__PAYMENTS__PROVIDERS__STRIPE__SECRET_KEY"], value = var.stripe_secret_key }
    "razorpay-secret" = { env = ["FEAST__PAYMENTS__PROVIDERS__RAZORPAY__KEY_SECRET"], value = var.razorpay_key_secret }
    "stripe-webhook"  = { env = ["FEAST__PAYMENTS__PROVIDERS__STRIPE__WEBHOOK_SECRET"], value = var.stripe_webhook_secret }
    "razorpay-webhook" = { env = ["FEAST__PAYMENTS__PROVIDERS__RAZORPAY__WEBHOOK_SECRET"], value = var.razorpay_webhook_secret }
    "twilio-token"    = { env = ["FEAST__AUTH__OTP__TWILIO__AUTH_TOKEN"], value = var.twilio_auth_token }
  }
  secret_env = flatten([for name, s in local.secrets : [for e in s.env : { name = e, secret = name }]])
  api_env = {
    FEAST_COUNTRY                                       = var.country
    FEAST_ENV                                           = var.environment
    FEAST__APP__SEED_ON_EMPTY                           = local.prod ? "false" : "true" # demo restaurants/diners in development only
    FEAST__HTTP__PORT                                   = "8080"
    FEAST__HTTP__PROXY_HOPS                             = "1"
    FEAST__BUS__KIND                                    = "redis"
    FEAST__BUS__REDIS__URL                              = "redis://${google_redis_instance.main.host}:${google_redis_instance.main.port}/0"
    FEAST__STORAGE__KIND                                = "s3"
    FEAST__STORAGE__S3__ENDPOINT                        = "https://storage.googleapis.com"
    FEAST__STORAGE__S3__BUCKET                          = google_storage_bucket.documents.name
    FEAST__STORAGE__S3__REGION                          = "auto"
    FEAST__STORAGE__S3__PATH_STYLE                      = "true"
    FEAST__STORAGE__S3__ACCESS_KEY_ID                   = google_storage_hmac_key.app.access_id
    FEAST__PAYMENTS__FORCE_PROVIDER                     = local.prod ? "" : "mock"
    FEAST__PAYMENTS__PROVIDERS__MOCK__ENABLED           = local.prod ? "false" : "true"
    FEAST__PAYMENTS__PROVIDERS__STRIPE__ENABLED         = var.stripe_secret_key != "" ? "true" : "false"
    FEAST__PAYMENTS__PROVIDERS__STRIPE__PUBLISHABLE_KEY = var.stripe_publishable_key
    FEAST__PAYMENTS__PROVIDERS__RAZORPAY__ENABLED       = var.razorpay_key_secret != "" ? "true" : "false"
    FEAST__PAYMENTS__PROVIDERS__RAZORPAY__KEY_ID        = var.razorpay_key_id
    FEAST__AUTH__OTP__PROVIDER                          = var.twilio_account_sid != "" ? "twilio" : "fixed"
    FEAST__AUTH__OTP__TWILIO__ACCOUNT_SID               = var.twilio_account_sid
    FEAST__AUTH__OTP__TWILIO__SERVICE_SID               = var.twilio_service_sid
  }
}

resource "google_secret_manager_secret" "app" {
  for_each  = local.secrets
  secret_id = "${local.name}-${each.key}"
  replication {
    auto {}
  }
  depends_on = [google_project_service.apis]
}

resource "google_secret_manager_secret_version" "app" {
  for_each    = local.secrets
  secret      = google_secret_manager_secret.app[each.key].id
  secret_data = each.value.value != "" ? each.value.value : "-"
}

resource "google_secret_manager_secret_iam_member" "app" {
  for_each  = local.secrets
  secret_id = google_secret_manager_secret.app[each.key].id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.app.email}"
}

# ------------------------------------------------------------------ app

resource "google_cloud_run_v2_service" "app" {
  name                = local.name
  location            = var.region
  ingress             = "INGRESS_TRAFFIC_ALL"
  deletion_protection = false

  template {
    service_account = google_service_account.app.email
    timeout         = "3600s" # live updates (SSE); browsers reconnect after this

    scaling {
      min_instance_count = var.min_instances
      max_instance_count = var.max_instances
    }

    vpc_access {
      network_interfaces {
        network    = google_compute_network.main.id
        subnetwork = google_compute_subnetwork.main.id
      }
      egress = "PRIVATE_RANGES_ONLY"
    }

    volumes {
      name = "cloudsql"
      cloud_sql_instance {
        instances = [google_sql_database_instance.main.connection_name]
      }
    }

    containers {
      name       = "web"
      image      = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.images.repository_id}/feast-web:${var.image_tag}"
      depends_on = ["api"]
      ports {
        container_port = 3000
      }
      env {
        name  = "FEAST_API_INTERNAL_URL"
        value = "http://localhost:8080"
      }
      env {
        name  = "FEAST_COUNTRY"
        value = var.country
      }
      dynamic "env" {
        for_each = var.legal
        content {
          name  = env.key
          value = env.value
        }
      }
      resources {
        limits   = { cpu = "1", memory = "512Mi" }
        cpu_idle = false # background work + live updates need CPU between requests
      }
      startup_probe {
        http_get {
          path = "/healthz"
          port = 3000
        }
      }
    }

    containers {
      name  = "api"
      image = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.images.repository_id}/feast-api:${var.image_tag}"
      args  = ["all"]
      dynamic "env" {
        for_each = merge(local.api_env, var.extra_env)
        content {
          name  = env.key
          value = env.value
        }
      }
      dynamic "env" {
        for_each = local.secret_env
        content {
          name = env.value.name
          value_source {
            secret_key_ref {
              secret  = google_secret_manager_secret.app[env.value.secret].secret_id
              version = "latest"
            }
          }
        }
      }
      volume_mounts {
        name       = "cloudsql"
        mount_path = "/cloudsql"
      }
      resources {
        limits   = { cpu = "1", memory = "1Gi" }
        cpu_idle = false
      }
      startup_probe {
        initial_delay_seconds = 5
        period_seconds        = 5
        failure_threshold     = 36
        http_get {
          path = "/healthz"
          port = 8080
        }
      }
    }
  }

  depends_on = [google_secret_manager_secret_version.app, google_secret_manager_secret_iam_member.app, google_project_iam_member.sql]
}

resource "google_cloud_run_v2_service_iam_member" "public" {
  name     = google_cloud_run_v2_service.app.name
  location = var.region
  role     = "roles/run.invoker"
  member   = "allUsers"
}
