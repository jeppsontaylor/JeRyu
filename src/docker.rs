//! Owner: Docker Control Plane subsystem
//! Proof: `cargo nextest run -p jeryu -- docker`
//! Invariants: Docker calls preserve container ownership labels and surface runtime errors to callers.
//! Docker runtime control for jeryu.
//!
//! Wraps bollard to manage runner-manager containers.

use anyhow::{Context, Result};
use bollard::Docker;

#[path = "docker_manager.rs"]
mod docker_manager;
#[path = "docker_volume.rs"]
mod docker_volume;

#[derive(Clone)]
pub struct DockerCtl {
    docker: Docker,
}

impl DockerCtl {
    pub fn connect() -> Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("connecting to Docker daemon")?;
        Ok(Self { docker })
    }
}
