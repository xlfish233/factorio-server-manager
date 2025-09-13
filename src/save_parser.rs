use crate::error::AppError;
use serde::Serialize;
use std::io::Read;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u32, u32, u32, u32);

impl Version {
    fn less(&self, other: Version) -> bool {
        (self.0, self.1, self.2, self.3) < (other.0, other.1, other.2, other.3)
    }
    fn ge(&self, other: Version) -> bool {
        !self.less(other)
    }
    fn greater(&self, other: Version) -> bool {
        (self.0, self.1, self.2, self.3) > (other.0, other.1, other.2, other.3)
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct SaveModEntry {
    pub name: String,
    pub version: String,
}

// Utilities mirroring Go reader helpers
fn read_exact<const N: usize>(r: &mut dyn Read) -> Result<[u8; N], AppError> {
    let mut buf = [0u8; N];
    r.read_exact(&mut buf).map_err(|e| AppError::Config {
        msg: format!("read {} bytes: {}", N, e),
    })?;
    Ok(buf)
}

fn read_u8(r: &mut dyn Read) -> Result<u8, AppError> {
    Ok(read_exact::<1>(r)?[0])
}
fn read_u16_le(r: &mut dyn Read) -> Result<u16, AppError> {
    Ok(u16::from_le_bytes(read_exact::<2>(r)?))
}
fn read_u32_le(r: &mut dyn Read) -> Result<u32, AppError> {
    Ok(u32::from_le_bytes(read_exact::<4>(r)?))
}

fn read_version64(r: &mut dyn Read) -> Result<Version, AppError> {
    let b = read_exact::<8>(r)?;
    Ok(Version(
        u16::from_le_bytes([b[0], b[1]]) as u32,
        u16::from_le_bytes([b[2], b[3]]) as u32,
        u16::from_le_bytes([b[4], b[5]]) as u32,
        u16::from_le_bytes([b[6], b[7]]) as u32,
    ))
}

fn read_optim_uint(r: &mut dyn Read, game: Version, bit_size: u8) -> Result<u32, AppError> {
    // Optimization used since >= 0.14.14
    if game.ge(Version(0, 14, 14, 0)) {
        let b = read_u8(r)?;
        if b != 0xFF {
            return Ok(b as u32);
        }
    }
    match bit_size {
        16 => read_u16_le(r).map(|v| v as u32),
        32 => read_u32_le(r),
        _ => Err(AppError::Config {
            msg: format!("unsupported bit_size {}", bit_size),
        }),
    }
}

fn read_version48(r: &mut dyn Read, game: Version) -> Result<(u32, u32, u32), AppError> {
    let a = read_optim_uint(r, game, 16)?;
    let b = read_optim_uint(r, game, 16)?;
    let c = read_optim_uint(r, game, 16)?;
    Ok((a, b, c))
}

fn read_string(r: &mut dyn Read, game: Version, force_optimized: bool) -> Result<String, AppError> {
    let n = if game.ge(Version(0, 16, 0, 0)) || force_optimized {
        read_optim_uint(r, game, 32)?
    } else {
        read_u32_le(r)?
    } as usize;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).map_err(|e| AppError::Config {
        msg: format!("failed to read string: {}", e),
    })?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

pub fn extract_mods_from_save_zip(zip_path: &str) -> Result<Vec<SaveModEntry>, AppError> {
    let file = std::fs::File::open(zip_path).map_err(|e| AppError::BadRequest {
        msg: format!("cannot open save zip: {}", e),
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| AppError::BadRequest {
        msg: format!("invalid zip: {}", e),
    })?;

    // Find first matching data file like Go: level.dat or level-init.dat; be permissive for script-init.dat too
    let mut idx: Option<usize> = None;
    for i in 0..zip.len() {
        if let Ok(f) = zip.by_index(i) {
            let name = f.name().rsplit('/').next().unwrap_or("").to_string();
            if name == "level.dat" || name == "level-init.dat" || name == "script-init.dat" {
                idx = Some(i);
                break;
            }
        }
    }
    let i = idx.ok_or_else(|| AppError::BadRequest {
        msg: "cannot find level.dat in save".into(),
    })?;
    let mut f = zip.by_index(i).map_err(|e| AppError::BadRequest {
        msg: format!("open entry: {}", e),
    })?;

    // Read header roughly like Go, enough to reach Mods list
    // FactorioVersion
    let reader = (&mut f) as &mut dyn Read;
    let fv = read_version64(reader)?;

    // and since >= 0.17 one random byte
    if fv.ge(Version(0, 17, 0, 0)) {
        let _ = read_u8(reader)?;
    }

    // campaign, name, base_mod
    let _campaign = read_string(reader, fv, false)?;
    let _name = read_string(reader, fv, false)?;
    let _base_mod = read_string(reader, fv, false)?;

    // difficulty
    let _ = read_u8(reader)?;

    // finished, player_won
    let _ = read_u8(reader)?;
    let _ = read_u8(reader)?;

    // next_level
    let _ = read_string(reader, fv, false)?;

    // 0.12+
    if fv.ge(Version(0, 12, 0, 0)) {
        let _ = read_u8(reader)?; // can_continue
        let _ = read_u8(reader)?; // finished_but_continuing
    }

    // saving_replay
    let _ = read_u8(reader)?;

    // 0.16+
    if fv.ge(Version(0, 16, 0, 0)) {
        let _ = read_u8(reader)?;
    }

    // loaded_from version48 + build
    let _ = read_version48(reader, fv)?;
    let _ = read_u16_le(reader)?; // loaded_from_build

    // allowed_commands and legacy adjust
    let mut _allowed = read_u8(reader)?;
    if fv.less(Version(0, 13, 0, 87)) {
        _allowed = if _allowed == 0 { 2 } else { 1 };
        let _ = _allowed; // ignore
    }

    // stats present before 0.13.0.42
    if fv.less(Version(0, 13, 0, 42)) {
        // very rough skip according to Go layout
        let n = read_u32_le(reader)?;
        for _ in 0..n {
            let _id = read_u8(reader)?;
            for _ in 0..3 {
                let len = read_u32_le(reader)?;
                for _ in 0..len {
                    let _key = read_u16_le(reader)?;
                    let _val = read_u32_le(reader)?;
                    let _ = (_key, _val);
                }
            }
        }
    }

    // number of mods
    let num_mods = if fv.ge(Version(0, 16, 0, 0)) {
        read_optim_uint(reader, fv, 32)?
    } else {
        read_u32_le(reader)?
    } as usize;

    let mut mods = Vec::with_capacity(num_mods);
    for _ in 0..num_mods {
        let name = read_string(reader, fv, true)?;
        let (ma, mi, pa) = read_version48(reader, fv)?;
        // Read CRC for > 0.15.0.91
        if fv.greater(Version(0, 15, 0, 91)) {
            let _ = read_u32_le(reader)?;
        }
        let version = format!("{}.{}.{}", ma, mi, pa);
        mods.push(SaveModEntry { name, version });
    }

    Ok(mods)
}
