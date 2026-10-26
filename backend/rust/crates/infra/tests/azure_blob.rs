//! Azure Blob adapter against Azurite (Microsoft's emulator). Skipped unless
//! FEAST_TEST_AZURITE is set, e.g. `npx azurite-blob --blobPort 10000` then
//! FEAST_TEST_AZURITE=http://127.0.0.1:10000/devstoreaccount1 cargo test --test azure_blob

use feast_application::ports::FileStorage;
use feast_infra::storage::{AzureBlobConfig, AzureBlobStorage};

#[tokio::test]
async fn round_trip_against_azurite() {
    let Ok(endpoint) = std::env::var("FEAST_TEST_AZURITE") else { return };
    let s = AzureBlobStorage::new(AzureBlobConfig {
        account: "devstoreaccount1".into(),
        container: "feast-test".into(),
        // Azurite's well-known development key.
        access_key: "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==".into(),
        sas_token: String::new(),
        endpoint,
        prefix: "au/".into(),
    })
    .unwrap();
    s.ensure_container().await.unwrap();
    s.health().await.unwrap();
    s.put("documents/r1/d1", b"%PDF-1.4 azure".to_vec(), "application/pdf").await.unwrap();
    let (bytes, ct) = s.get("documents/r1/d1").await.unwrap();
    assert_eq!(bytes, b"%PDF-1.4 azure");
    assert_eq!(ct, "application/pdf");
    assert_eq!(s.get("documents/r1/missing").await.unwrap_err().code, "NOT_FOUND");
}
