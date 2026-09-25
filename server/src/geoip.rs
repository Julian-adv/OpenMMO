use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::path::Path;
use tracing::{info, warn};

pub const UNKNOWN_COUNTRY: &str = "ZZ";

type Ranges<T> = Vec<(T, T, [u8; 2])>;

/// DB-IP country ranges (`start,end,CC` CSV), sorted for one binary search per connection.
#[derive(Default)]
pub struct GeoIp {
    v4: Ranges<u32>,
    v6: Ranges<u128>,
}

impl GeoIp {
    /// A missing database only costs the country metric, so the server still starts.
    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::File::open(path).and_then(|file| Self::parse(BufReader::new(file))) {
            Ok(geoip) => {
                info!("GeoIP database loaded: {} ranges", geoip.ranges());
                geoip
            }
            Err(error) => {
                warn!(
                    "GeoIP database {} not loaded ({error}); sessions are recorded as country {UNKNOWN_COUNTRY}",
                    path.display()
                );
                Self::default()
            }
        }
    }

    fn parse(mut reader: impl BufRead) -> std::io::Result<Self> {
        let mut geoip = Self::default();
        let mut line = String::new();
        while {
            line.clear();
            reader.read_line(&mut line)? > 0
        } {
            let mut fields = line.trim().splitn(3, ',');
            let (Some(start), Some(end), Some(country)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let &[a, b] = country.as_bytes() else {
                continue;
            };
            if !a.is_ascii_uppercase() || !b.is_ascii_uppercase() {
                continue;
            }
            match (start.parse(), end.parse()) {
                (Ok(IpAddr::V4(start)), Ok(IpAddr::V4(end))) => {
                    geoip.v4.push((start.into(), end.into(), [a, b]));
                }
                (Ok(IpAddr::V6(start)), Ok(IpAddr::V6(end))) => {
                    geoip.v6.push((start.into(), end.into(), [a, b]));
                }
                _ => {}
            }
        }
        geoip.v4.sort_unstable();
        geoip.v6.sort_unstable();
        geoip.v4.shrink_to_fit();
        geoip.v6.shrink_to_fit();
        Ok(geoip)
    }

    fn ranges(&self) -> usize {
        self.v4.len() + self.v6.len()
    }

    /// ISO 3166-1 alpha-2 code, or `ZZ` when the address is private or not in the database.
    pub fn country(&self, ip: IpAddr) -> String {
        let code = match ip.to_canonical() {
            IpAddr::V4(ip) => find(&self.v4, ip.into()),
            IpAddr::V6(ip) => find(&self.v6, ip.into()),
        };
        code.as_ref()
            .and_then(|code| std::str::from_utf8(code).ok())
            .unwrap_or(UNKNOWN_COUNTRY)
            .to_owned()
    }
}

fn find<T: Ord + Copy>(ranges: &[(T, T, [u8; 2])], ip: T) -> Option<[u8; 2]> {
    let index = ranges
        .partition_point(|range| range.0 <= ip)
        .checked_sub(1)?;
    let (_, end, country) = ranges[index];
    (ip <= end).then_some(country)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
1.0.0.0,1.0.0.255,AU
0.0.0.0,0.255.255.255,ZZ
220.89.0.0,220.89.255.255,KR
bad line
10.0.0.0,10.255.255.255,ZZ
2001:200::,2001:200:ffff:ffff:ffff:ffff:ffff:ffff,JP
1.1.1.0,1.1.1.255,toolong
";

    fn sample() -> GeoIp {
        GeoIp::parse(SAMPLE.as_bytes()).expect("sample parses")
    }

    #[test]
    fn looks_up_unsorted_ranges_and_skips_malformed_lines() {
        let geoip = sample();
        assert_eq!(geoip.ranges(), 5);
        assert_eq!(geoip.country("220.89.217.201".parse().expect("ip")), "KR");
        assert_eq!(geoip.country("1.0.0.255".parse().expect("ip")), "AU");
        assert_eq!(geoip.country("2001:200::1".parse().expect("ip")), "JP");
        assert_eq!(
            geoip.country("::ffff:220.89.0.1".parse().expect("ip")),
            "KR"
        );
    }

    /// `GEOIP_DB=data/geoip/dbip-country-lite.csv cargo test -p onlinerpg-server geoip -- --ignored`
    #[test]
    #[ignore = "needs the downloaded database"]
    fn downloaded_database_resolves_known_addresses() {
        let path = std::env::var("GEOIP_DB").expect("GEOIP_DB");
        let geoip = GeoIp::load_or_default(Path::new(&path));
        assert!(geoip.ranges() > 100_000);
        assert_eq!(geoip.country("220.89.217.201".parse().expect("ip")), "KR");
        assert_eq!(geoip.country("8.8.8.8".parse().expect("ip")), "US");
        assert_eq!(geoip.country("2001:220::1".parse().expect("ip")), "KR");
        assert_eq!(
            geoip.country("127.0.0.1".parse().expect("ip")),
            UNKNOWN_COUNTRY
        );
    }

    #[test]
    fn gaps_private_ranges_and_an_empty_database_are_unknown() {
        let geoip = sample();
        for ip in ["1.0.1.0", "10.1.2.3", "127.0.0.1", "::1", "2001:201::1"] {
            assert_eq!(geoip.country(ip.parse().expect("ip")), UNKNOWN_COUNTRY);
        }
        assert_eq!(
            GeoIp::default().country("220.89.217.201".parse().expect("ip")),
            UNKNOWN_COUNTRY
        );
    }
}
