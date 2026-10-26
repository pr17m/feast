output "url" {
  value = "https://${azurerm_container_app.app.ingress[0].fqdn}"
}

output "registry_name" {
  value = azurerm_container_registry.main.name
}

output "resource_group" {
  value = azurerm_resource_group.main.name
}

output "seed_password" {
  description = "Password for the seeded admin/owner accounts (admin@feast.demo …)."
  value       = random_password.seed.result
  sensitive   = true
}

output "logs" {
  value = "az containerapp logs show -n ${azurerm_container_app.app.name} -g ${azurerm_resource_group.main.name} --container api --follow"
}
