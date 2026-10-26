# Feast on Azure — one country per stack (Terraform workspace per country).
#
#   Internet → Container App ingress (HTTPS) → [web :3000 → api :8080 (localhost)]
#   api → PostgreSQL Flexible Server, Azure Cache for Redis (TLS), Blob Storage
#
# Images live in this stack's Azure Container Registry (built with `az acr build`).

locals {
  name   = "feast-${var.country}"
  prod   = var.environment == "production"
  suffix = random_string.suffix.result
}

resource "random_string" "suffix" {
  length  = 6
  special = false
  upper   = false
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

resource "azurerm_resource_group" "main" {
  name     = "rg-${local.name}"
  location = var.location
  tags     = { app = "feast", country = var.country }
}

# ------------------------------------------------------------------ images

resource "azurerm_container_registry" "main" {
  name                = "feast${var.country}${local.suffix}"
  resource_group_name = azurerm_resource_group.main.name
  location            = azurerm_resource_group.main.location
  sku                 = "Basic"
  admin_enabled       = false
}

resource "azurerm_user_assigned_identity" "app" {
  name                = "id-${local.name}"
  resource_group_name = azurerm_resource_group.main.name
  location            = azurerm_resource_group.main.location
}

resource "azurerm_role_assignment" "pull" {
  scope                = azurerm_container_registry.main.id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.app.principal_id
}

# ------------------------------------------------------------------ data

resource "azurerm_postgresql_flexible_server" "main" {
  name                          = "psql-${local.name}-${local.suffix}"
  resource_group_name           = azurerm_resource_group.main.name
  location                      = azurerm_resource_group.main.location
  version                       = "16"
  administrator_login           = "feast"
  administrator_password        = random_password.db.result
  sku_name                      = var.postgres_sku
  storage_mb                    = 32768
  backup_retention_days         = 7
  public_network_access_enabled = true # restricted to Azure services by the firewall rule below; TLS required
  lifecycle {
    ignore_changes = [zone, high_availability]
  }
}

resource "azurerm_postgresql_flexible_server_firewall_rule" "azure" {
  name             = "allow-azure-services"
  server_id        = azurerm_postgresql_flexible_server.main.id
  start_ip_address = "0.0.0.0"
  end_ip_address   = "0.0.0.0"
}

resource "azurerm_postgresql_flexible_server_database" "feast" {
  name      = "feast"
  server_id = azurerm_postgresql_flexible_server.main.id
  charset   = "UTF8"
  collation = "en_US.utf8"
}

resource "azurerm_redis_cache" "main" {
  name                 = "redis-${local.name}-${local.suffix}"
  resource_group_name  = azurerm_resource_group.main.name
  location             = azurerm_resource_group.main.location
  capacity             = 0
  family               = "C"
  sku_name             = var.redis_sku
  minimum_tls_version  = "1.2"
  non_ssl_port_enabled = false
}

resource "azurerm_storage_account" "documents" {
  name                            = "feast${var.country}docs${local.suffix}"
  resource_group_name             = azurerm_resource_group.main.name
  location                        = azurerm_resource_group.main.location
  account_tier                    = "Standard"
  account_replication_type        = "LRS"
  min_tls_version                 = "TLS1_2"
  allow_nested_items_to_be_public = false
}

resource "azurerm_storage_container" "documents" {
  name                  = "documents"
  storage_account_id    = azurerm_storage_account.documents.id
  container_access_type = "private"
}

resource "azurerm_management_lock" "data" {
  for_each   = var.protect_data ? { db = azurerm_postgresql_flexible_server.main.id, docs = azurerm_storage_account.documents.id } : {}
  name       = "protect-${each.key}"
  scope      = each.value
  lock_level = "CanNotDelete"
}

# ------------------------------------------------------------------ app

resource "azurerm_log_analytics_workspace" "main" {
  name                = "log-${local.name}"
  resource_group_name = azurerm_resource_group.main.name
  location            = azurerm_resource_group.main.location
  sku                 = "PerGB2018"
  retention_in_days   = 30
}

resource "azurerm_container_app_environment" "main" {
  name                       = "cae-${local.name}"
  resource_group_name        = azurerm_resource_group.main.name
  location                   = azurerm_resource_group.main.location
  log_analytics_workspace_id = azurerm_log_analytics_workspace.main.id
}

locals {
  pg_host = azurerm_postgresql_flexible_server.main.fqdn
  db_url  = "postgres://feast:${random_password.db.result}@${local.pg_host}:5432/feast?sslmode=require"
  # Secrets (container-app secret name => env var). Empty optional keys become "-".
  secrets = {
    "database-url"    = { env = ["FEAST__DATABASE__URL", "FEAST__NOTIFICATIONS__DATABASE_URL"], value = local.db_url }
    "redis-url"       = { env = ["FEAST__BUS__REDIS__URL"], value = "rediss://:${urlencode(azurerm_redis_cache.main.primary_access_key)}@${azurerm_redis_cache.main.hostname}:6380/0" }
    "jwt-secret"      = { env = ["FEAST__AUTH__JWT_SECRET"], value = random_password.jwt.result }
    "seed-password"   = { env = ["FEAST__APP__SEED_PASSWORD", "FEAST__APP__INITIAL_PARTNER_PASSWORD"], value = random_password.seed.result }
    "storage-key"     = { env = ["FEAST__STORAGE__AZURE__ACCESS_KEY"], value = azurerm_storage_account.documents.primary_access_key }
    "stripe-secret"   = { env = ["FEAST__PAYMENTS__PROVIDERS__STRIPE__SECRET_KEY"], value = var.stripe_secret_key }
    "razorpay-secret" = { env = ["FEAST__PAYMENTS__PROVIDERS__RAZORPAY__KEY_SECRET"], value = var.razorpay_key_secret }
    "stripe-webhook"  = { env = ["FEAST__PAYMENTS__PROVIDERS__STRIPE__WEBHOOK_SECRET"], value = var.stripe_webhook_secret }
    "razorpay-webhook" = { env = ["FEAST__PAYMENTS__PROVIDERS__RAZORPAY__WEBHOOK_SECRET"], value = var.razorpay_webhook_secret }
    "twilio-token"    = { env = ["FEAST__AUTH__OTP__TWILIO__AUTH_TOKEN"], value = var.twilio_auth_token }
    "fcm-credentials" = { env = ["FEAST__PUSH__FCM__CREDENTIALS_JSON"], value = var.fcm_credentials_json }
    "review-code"     = { env = ["FEAST__AUTH__OTP__REVIEW_CODE"], value = var.review_code }
  }
  secret_env = flatten([for name, s in local.secrets : [for e in s.env : { name = e, secret = name }]])
  api_env = {
    FEAST_COUNTRY                                       = var.country
    FEAST_ENV                                           = var.environment
    FEAST__APP__SEED_ON_EMPTY                           = local.prod ? "false" : "true" # demo restaurants/diners in development only
    FEAST__HTTP__PORT                                   = "8080"
    FEAST__HTTP__PROXY_HOPS                             = "1"
    FEAST__BUS__KIND                                    = "redis"
    FEAST__STORAGE__KIND                                = "azure"
    FEAST__STORAGE__AZURE__ACCOUNT                      = azurerm_storage_account.documents.name
    FEAST__STORAGE__AZURE__CONTAINER                    = azurerm_storage_container.documents.name
    FEAST__PAYMENTS__FORCE_PROVIDER                     = local.prod ? "" : "mock"
    FEAST__PAYMENTS__PROVIDERS__MOCK__ENABLED           = local.prod ? "false" : "true"
    FEAST__PAYMENTS__PROVIDERS__STRIPE__ENABLED         = var.stripe_secret_key != "" ? "true" : "false"
    FEAST__PAYMENTS__PROVIDERS__STRIPE__PUBLISHABLE_KEY = var.stripe_publishable_key
    FEAST__PAYMENTS__PROVIDERS__RAZORPAY__ENABLED       = var.razorpay_key_secret != "" ? "true" : "false"
    FEAST__PAYMENTS__PROVIDERS__RAZORPAY__KEY_ID        = var.razorpay_key_id
    FEAST__AUTH__OTP__PROVIDER                          = var.twilio_account_sid != "" ? "twilio" : "fixed"
    FEAST__AUTH__OTP__TWILIO__ACCOUNT_SID               = var.twilio_account_sid
    FEAST__AUTH__OTP__TWILIO__SERVICE_SID               = var.twilio_service_sid
    FEAST__PUSH__PROVIDER                               = var.fcm_credentials_json != "" ? "fcm" : ""
    FEAST__AUTH__OTP__REVIEW_NUMBERS                    = join(",", var.review_numbers)
  }
  # Phone app links (/.well-known/*), read by the web app.
  web_app_links = {
    FEAST_IOS_APP_IDS  = join(",", var.ios_app_ids)
    FEAST_ANDROID_APPS = join(",", var.android_apps)
  }
}

resource "azurerm_container_app" "app" {
  name                         = local.name
  resource_group_name          = azurerm_resource_group.main.name
  container_app_environment_id = azurerm_container_app_environment.main.id
  revision_mode                = "Single"

  identity {
    type         = "UserAssigned"
    identity_ids = [azurerm_user_assigned_identity.app.id]
  }

  registry {
    server   = azurerm_container_registry.main.login_server
    identity = azurerm_user_assigned_identity.app.id
  }

  dynamic "secret" {
    for_each = local.secrets
    content {
      name  = secret.key
      value = secret.value.value != "" ? secret.value.value : "-"
    }
  }

  ingress {
    external_enabled = true
    target_port      = 3000
    transport        = "auto"
    traffic_weight {
      latest_revision = true
      percentage      = 100
    }
  }

  template {
    min_replicas = var.min_replicas
    max_replicas = var.max_replicas

    container {
      name   = "api"
      image  = "${azurerm_container_registry.main.login_server}/feast-api:${var.image_tag}"
      args   = ["all"]
      cpu    = 0.5
      memory = "1Gi"
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
          name        = env.value.name
          secret_name = env.value.secret
        }
      }
      liveness_probe {
        transport               = "HTTP"
        port                    = 8080
        path                    = "/healthz"
        initial_delay           = 20
        interval_seconds        = 15
        failure_count_threshold = 4
      }
    }

    container {
      name   = "web"
      image  = "${azurerm_container_registry.main.login_server}/feast-web:${var.image_tag}"
      cpu    = 0.5
      memory = "1Gi"
      env {
        name  = "FEAST_API_INTERNAL_URL"
        value = "http://localhost:8080"
      }
      env {
        name  = "FEAST_COUNTRY"
        value = var.country
      }
      dynamic "env" {
        for_each = merge(var.legal, local.web_app_links)
        content {
          name  = env.key
          value = env.value
        }
      }
      liveness_probe {
        transport = "HTTP"
        port      = 3000
        path      = "/healthz"
      }
    }
  }

  depends_on = [azurerm_role_assignment.pull, azurerm_postgresql_flexible_server_firewall_rule.azure, azurerm_postgresql_flexible_server_database.feast]
}
