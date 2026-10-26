use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GptIdentity {
    pub size_bytes: u64,
    pub sector_size: u64,
    pub primary: Vec<u8>,
    pub backup: Vec<u8>,
    pub entries: Vec<u8>,
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("GPT header length validated"),
    )
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("GPT header length validated"),
    )
}

fn crc(bytes: &[u8]) -> u32 {
    let mut value = !0u32;
    for byte in bytes {
        value ^= *byte as u32;
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(value & 1));
        }
    }
    !value
}

fn read_at(disk: &mut (impl Read + Seek), offset: u64, length: usize) -> Result<Vec<u8>> {
    let mut bytes = vec![0; length];
    disk.seek(SeekFrom::Start(offset))?;
    disk.read_exact(&mut bytes)?;
    Ok(bytes)
}

impl GptIdentity {
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.sector_size, 512 | 4096),
            "unsupported GPT sector size"
        );
        ensure!(
            self.size_bytes % self.sector_size == 0 && self.size_bytes / self.sector_size >= 68,
            "invalid GPT disk size"
        );
        let last = self.size_bytes / self.sector_size - 1;
        for (header, current, alternate, table) in [
            (&self.primary, 1, last, 2),
            (&self.backup, last, 1, last - 32 * 512 / self.sector_size),
        ] {
            ensure!(
                header.len() == self.sector_size as usize && &header[..8] == b"EFI PART",
                "invalid GPT header"
            );
            ensure!(
                u32_at(header, 8) == 0x10000 && u32_at(header, 12) == 92,
                "unsupported GPT header revision or size"
            );
            let mut checked = header[..92].to_vec();
            checked[16..20].fill(0);
            ensure!(
                crc(&checked) == u32_at(header, 16),
                "invalid GPT header checksum"
            );
            ensure!(
                u64_at(header, 24) == current
                    && u64_at(header, 32) == alternate
                    && u64_at(header, 72) == table,
                "unsupported GPT table location"
            );
            ensure!(
                u32_at(header, 80) == 128
                    && u32_at(header, 84) == 128
                    && self.entries.len() == 16384,
                "unsupported GPT entry layout"
            );
            ensure!(
                crc(&self.entries) == u32_at(header, 88),
                "invalid GPT partition checksum"
            );
            ensure!(
                u64_at(header, 40) >= 2 + 16384 / self.sector_size
                    && u64_at(header, 48) < last - 16384 / self.sector_size
                    && u64_at(header, 40) <= u64_at(header, 48),
                "invalid GPT usable range"
            );
        }
        ensure!(
            self.primary[40..72] == self.backup[40..72],
            "GPT headers disagree"
        );
        Ok(())
    }
}

pub fn capture(disk: &mut (impl Read + Seek), size: u64, sector: u64) -> Result<GptIdentity> {
    ensure!(
        matches!(sector, 512 | 4096) && size % sector == 0 && size / sector >= 68,
        "invalid GPT disk geometry"
    );
    let saved = GptIdentity {
        size_bytes: size,
        sector_size: sector,
        primary: read_at(disk, sector, sector as usize)?,
        backup: read_at(disk, size - sector, sector as usize)?,
        entries: read_at(disk, 2 * sector, 16384)?,
    };
    saved.validate()?;
    ensure!(
        read_at(disk, size - sector - 16384, 16384)? == saved.entries,
        "GPT partition tables disagree"
    );
    Ok(saved)
}

pub fn restore(disk: &mut (impl Read + Write + Seek), saved: &GptIdentity) -> Result<()> {
    saved.validate()?;
    let current = capture(disk, saved.size_bytes, saved.sector_size)?;
    for (before, after) in [
        (&saved.primary, &current.primary),
        (&saved.backup, &current.backup),
    ] {
        let mut before = before.clone();
        let mut after = after.clone();
        for range in [16..20, 56..72, 88..92] {
            before[range.clone()].fill(0);
            after[range].fill(0);
        }
        ensure!(
            before == after,
            "GPT layout changed during servicing; refusing identity restore"
        );
    }
    let mut before = saved.entries.clone();
    let mut after = current.entries;
    for entry in 0..128 {
        before[entry * 128 + 16..entry * 128 + 32].fill(0);
        after[entry * 128 + 16..entry * 128 + 32].fill(0);
    }
    ensure!(
        before == after,
        "partition layout changed during servicing; refusing identity restore"
    );
    // Windows may regenerate partition GUIDs as well as the colliding disk GUID.
    for (offset, bytes) in [
        (saved.size_bytes - saved.sector_size - 16384, &saved.entries),
        (saved.size_bytes - saved.sector_size, &saved.backup),
        (2 * saved.sector_size, &saved.entries),
        (saved.sector_size, &saved.primary),
    ] {
        disk.seek(SeekFrom::Start(offset))?;
        disk.write_all(bytes)?;
    }
    disk.flush()?;
    ensure!(
        capture(disk, saved.size_bytes, saved.sector_size)? == *saved,
        "GPT identity restore verification failed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn crc(data: &[u8]) -> u32 {
        let mut value = !0u32;
        for byte in data {
            value ^= *byte as u32;
            for _ in 0..8 {
                value = (value >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(value & 1));
            }
        }
        !value
    }

    fn fixture(guid: u8) -> Cursor<Vec<u8>> {
        fixture_with_sector(guid, 512)
    }

    fn fixture_with_sector(guid: u8, sector: usize) -> Cursor<Vec<u8>> {
        let mut bytes = vec![0; sector * 128];
        let mut entries = vec![0u8; 128 * 128];
        entries[0] = 1;
        entries[16..32].fill(guid);
        entries[32..40].copy_from_slice(&34u64.to_le_bytes());
        entries[40..48].copy_from_slice(&80u64.to_le_bytes());
        for (lba, alternate, table) in [(1u64, 127u64, 2u64), (127, 1, 127 - 16384 / sector as u64)]
        {
            let mut header = vec![0; sector];
            header[..8].copy_from_slice(b"EFI PART");
            header[8..12].copy_from_slice(&0x10000u32.to_le_bytes());
            header[12..16].copy_from_slice(&92u32.to_le_bytes());
            header[24..32].copy_from_slice(&lba.to_le_bytes());
            header[32..40].copy_from_slice(&alternate.to_le_bytes());
            header[40..48].copy_from_slice(&34u64.to_le_bytes());
            header[48..56].copy_from_slice(&94u64.to_le_bytes());
            header[56..72].fill(guid);
            header[72..80].copy_from_slice(&table.to_le_bytes());
            header[80..84].copy_from_slice(&128u32.to_le_bytes());
            header[84..88].copy_from_slice(&128u32.to_le_bytes());
            header[88..92].copy_from_slice(&crc(&entries).to_le_bytes());
            let checksum = crc(&header[..92]);
            header[16..20].copy_from_slice(&checksum.to_le_bytes());
            bytes[lba as usize * sector..(lba as usize + 1) * sector].copy_from_slice(&header);
            bytes[table as usize * sector..table as usize * sector + entries.len()]
                .copy_from_slice(&entries);
        }
        Cursor::new(bytes)
    }

    #[test]
    fn restores_partition_and_disk_guids_without_touching_other_sectors() {
        for sector in [512, 4096] {
            let mut original = fixture_with_sector(1, sector);
            let saved = capture(&mut original, sector as u64 * 128, sector as u64).unwrap();
            let mut changed = fixture_with_sector(2, sector);
            changed.get_mut()[40 * sector] = 123;
            restore(&mut changed, &saved).unwrap();
            assert_eq!(
                capture(&mut changed, sector as u64 * 128, sector as u64).unwrap(),
                saved
            );
            assert_eq!(changed.get_ref()[40 * sector], 123);
        }
    }

    #[test]
    fn refuses_corruption_or_partition_layout_changes_before_writing() {
        let saved = capture(&mut fixture(1), 512 * 128, 512).unwrap();
        let mut changed = fixture(2);
        changed.get_mut()[2 * 512 + 32] = 35;
        let before = changed.get_ref().clone();
        assert!(restore(&mut changed, &saved).is_err());
        assert_eq!(changed.get_ref(), &before);
        let mut invalid = saved.clone();
        invalid.primary[72..80].copy_from_slice(&40u64.to_le_bytes());
        assert!(restore(&mut fixture(2), &invalid).is_err());
        assert!(capture(&mut fixture(1), 512 * 128, 1).is_err());
        let mut changed = fixture(2);
        let mut identity = capture(&mut changed, 512 * 128, 512).unwrap();
        identity.entries[32..40].copy_from_slice(&35u64.to_le_bytes());
        let checksum = crc(&identity.entries);
        for header in [&mut identity.primary, &mut identity.backup] {
            header[88..92].copy_from_slice(&checksum.to_le_bytes());
            header[16..20].fill(0);
            let checksum = crc(&header[..92]);
            header[16..20].copy_from_slice(&checksum.to_le_bytes());
        }
        changed.get_mut()[512..1024].copy_from_slice(&identity.primary);
        changed.get_mut()[127 * 512..128 * 512].copy_from_slice(&identity.backup);
        changed.get_mut()[2 * 512..2 * 512 + 16384].copy_from_slice(&identity.entries);
        changed.get_mut()[95 * 512..95 * 512 + 16384].copy_from_slice(&identity.entries);
        let before = changed.get_ref().clone();
        assert!(restore(&mut changed, &saved).is_err());
        assert_eq!(changed.get_ref(), &before);
    }
}
