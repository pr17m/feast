//! Generated code for the protos in `/protos`. Do not edit — change the .proto files.

pub mod feast {
    pub mod common {
        pub mod v1 {
            tonic::include_proto!("feast.common.v1");
        }
    }
    pub mod notification {
        pub mod v1 {
            tonic::include_proto!("feast.notification.v1");
        }
    }
}

pub use feast::common::v1 as common;
pub use feast::notification::v1 as notification;
