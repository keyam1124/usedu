use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;

pub fn allocated_bytes(metadata: &Metadata) -> u64 {
    metadata.blocks().saturating_mul(512)
}

pub fn device_id(metadata: &Metadata) -> u64 {
    metadata.dev()
}

pub fn inode_id(metadata: &Metadata) -> u64 {
    metadata.ino()
}

pub fn link_count(metadata: &Metadata) -> u64 {
    metadata.nlink()
}
