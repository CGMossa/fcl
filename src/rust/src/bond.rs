use chrono::{DateTime, Datelike, NaiveDate, Utc};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct FixedBond {
    value_date: NaiveDate,
    mty_date: NaiveDate,
    redem_value: f64,
    cpn_rate: f64,
    cpn_freq: u32,
}

#[derive(Debug)]
pub struct BondVal {
    pub ytm: f64,
    pub macd: f64,
    pub modd: f64,
}

#[derive(Debug)]
struct Cashflow {
    data: BTreeMap<NaiveDate, f64>,
}
impl Cashflow {
    fn size(&self) -> usize {
        self.data.len()
    }
    fn new() -> Cashflow {
        let data: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        return Cashflow { data };
    }
    fn cf(&self, ref_date: &NaiveDate, price: f64) -> Cashflow {
        if self.size() == 0 {
            return Cashflow::new();
        }
        let mut data: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        data.insert(*ref_date, -price);
        for (k, v) in &self.data {
            if k > ref_date {
                data.insert(*k, *v);
            }
        }
        Cashflow { data }
    }
    fn xirr_cf(&self) -> (Vec<DateTime<Utc>>, Vec<f64>) {
        let mut cfs: Vec<f64> = Vec::new();
        let mut dates: Vec<DateTime<Utc>> = Vec::new();
        for (k, v) in &self.data {
            cfs.push(*v);
            dates.push(DateTime::<Utc>::from_utc(k.and_hms(0, 0, 0), Utc));
        }
        (dates, cfs)
    }
}

impl FixedBond {
    pub fn new(
        value_date: NaiveDate,
        mty_date: NaiveDate,
        redem_value: f64,
        cpn_rate: f64,
        cpn_freq: u32,
    ) -> FixedBond {
        FixedBond {
            value_date,
            mty_date,
            redem_value,
            cpn_rate,
            cpn_freq,
        }
    }
    fn years(d1: &NaiveDate, d0: &NaiveDate) -> f64 {
        (d1.year() - d0.year()) as f64
        // must be as f64 first, otherwise u32 - u32 may overflow (when negative)
            + (d1.month() as f64 - d0.month() as f64) / 12.0
            + (d1.day() as f64 - d0.day() as f64) / 365.0
    }
    fn add_months(ref_date: &NaiveDate, months: u32) -> NaiveDate {
        let num_of_months = ref_date.year() * 12 + ref_date.month() as i32 + months as i32;
        let year = (num_of_months - 1) / 12;
        let month = (num_of_months - 1) % 12 + 1;
        let since = NaiveDate::signed_duration_since;
        let nxt_month = if month == 12 {
            NaiveDate::from_ymd(year + 1, 1 as u32, 1)
        } else {
            NaiveDate::from_ymd(year, (month + 1) as u32, 1)
        };
        let max_day = since(
            nxt_month,
            NaiveDate::from_ymd(year, month as u32, 1),
        )
        .num_days() as u32;
        let day = ref_date.day();
        NaiveDate::from_ymd(
            year,
            month as u32,
            if day > max_day { max_day } else { day },
        )
    }
    fn cpn_dates(&self, adjust: bool) -> Vec<NaiveDate> {
        let mut dates: Vec<NaiveDate> = vec![self.value_date];
        let mut ref_date = self.value_date;
        loop {
            match self.nxt_cpn_date(&ref_date, adjust) {
                Some(date) => {
                    ref_date = date;
                    dates.push(date);
                }
                None => break,
            }
        }
        dates
    }
    /// Calculate the Next Coupon Date
    /// @param adjust when true, it unadjust the last coupon date to mty date, if it's beyond
    fn nxt_cpn_date(&self, ref_date: &NaiveDate, adjust: bool) -> Option<NaiveDate> {
        if ref_date >= &self.mty_date {
            return None;
        }
        let res = match self.cpn_freq {
            1 => Some(FixedBond::add_months(ref_date, 12)),
            2 => Some(FixedBond::add_months(ref_date, 6)),
            4 => Some(FixedBond::add_months(ref_date, 3)),
            12 => Some(FixedBond::add_months(ref_date, 1)),
            0 => Some(self.mty_date),
            other => panic!("unexpected cpn_freq {}", other),
        };
        match res {
            Some(date) => {
                if date > self.mty_date && adjust {
                    Some(self.mty_date)
                } else {
                    Some(date)
                }
            }
            None => None,
        }
    }
    fn cpn_value(&self) -> f64 {
        let factor = match self.cpn_freq {
            1 => 1.0,
            2 => 0.5,
            4 => 0.25,
            12 => 1.0 / 12.0,
            0 => FixedBond::years(&self.mty_date, &self.value_date),
            other => panic!("unexpected cpn_freq {}", other),
        };
        self.redem_value * self.cpn_rate * factor
    }
    /// Calculate the accrued coupon
    /// @param eop, if true, at the coupon / mty date it returns 0 otherwise returns the paying coupon
    fn accrued(&self, ref_date: &NaiveDate, eop: bool) -> f64 {
        if ref_date > &self.mty_date || ref_date <= &self.value_date {
            return 0.0;
        }
        if eop && ref_date == &self.mty_date {
            return 0.0;
        }
        let cpn_dates = self.cpn_dates(false);
        let calculate = |i: usize| {
            // dbg!(&cpn_dates); dbg!(&ref_date); dbg!(i);
            let last_cpn_date = cpn_dates[i - 1];
            let nxt_cpn_date = cpn_dates[i];
            let cpn_days = nxt_cpn_date.signed_duration_since(last_cpn_date).num_days();
            let days = ref_date.signed_duration_since(last_cpn_date).num_days();
            // dbg!(cpn_days); dbg!(days);
            self.cpn_value() / cpn_days as f64 * days as f64
        };

        match cpn_dates.binary_search(&ref_date) {
            // when ok, it means it's one of the cpn date and the coupon has been paid then should be zero
            Ok(i) => if eop { 0.0 } else { calculate(i) },
            Err(i) => calculate(i),
        }
    }
    fn dirty_price(&self, ref_date: &NaiveDate, clean_price: f64) -> f64 {
        clean_price + self.accrued(ref_date, true)
    }
    fn cashflow(&self) -> Cashflow {
        let mut ref_date = self.nxt_cpn_date(&self.value_date, true);
        let mut res: Cashflow = Cashflow::new();
        loop {
            match ref_date {
                Some(date) => {
                    let value: f64 = if date == self.mty_date {
                        self.redem_value
                    } else {
                        0.0
                    } + self.accrued(&date, false);
                    res.data.insert(date, value);
                    ref_date = self.nxt_cpn_date(&date, true);
                }
                None => break,
            }
        }
        res
    }
    pub fn result(&self, ref_date: &NaiveDate, clean_price: f64) -> BondVal {
        let dirty_price = self.dirty_price(ref_date, clean_price);
        let cf = self.cashflow().cf(ref_date, dirty_price).xirr_cf();
        let ytm = financial::xirr(&cf.1, &cf.0, None).unwrap();
        let ytm_chg = 1e-6;
        let npv1 = financial::xnpv(ytm + ytm_chg, &cf.1, &cf.0).unwrap();
        let npv0 = financial::xnpv(ytm - ytm_chg, &cf.1, &cf.0).unwrap();
        let modd = -(npv1 - npv0) / (2.0 * ytm_chg * dirty_price);
        let cf2 = self.cashflow().cf(ref_date, dirty_price);
        let years: Vec<f64> = cf2
            .data
            .keys()
            .map(|date: &NaiveDate| FixedBond::years(date, ref_date))
            .collect();
        let macd = &years
            .iter()
            .zip(&cf.1)
            .map(|(t, cf)| cf * t * (1.0 + ytm).powf(-t))
            .sum()
            / dirty_price;
        BondVal { ytm, macd, modd }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn round(x: f64, digit: Option<u32>) -> f64 {
        let digit = digit.unwrap_or(4);
        let scale: f64 = 10f64.powf(digit as f64);
        (x * scale).round() / scale
    }
    fn rnd(x: f64) -> f64 {
        round(x, Some(3))
    }
    fn rnd2(x: f64) -> f64 {
        round(x, Some(2))
    }
    #[test]
    fn dirty_price() {
        let mut bond = FixedBond::new(
            NaiveDate::from_ymd(2010, 1, 1),
            NaiveDate::from_ymd(2015, 1, 1),
            100.0,
            0.05,
            2,
        );
        let ref_date = NaiveDate::from_ymd(2010, 1, 1);
        assert_eq!(bond.accrued(&ref_date, true), 0.0);
        let ref_date = NaiveDate::from_ymd(2011, 7, 1);
        assert_eq!(bond.dirty_price(&ref_date, 100.0), 100.0);
        let ref_date = NaiveDate::from_ymd(2011, 1, 1);
        assert_eq!(bond.dirty_price(&ref_date, 100.0), 100.0);
        assert_eq!(bond.accrued(&ref_date, false), 2.5);

        bond.cpn_freq = 1;
        let ref_date = NaiveDate::from_ymd(2010, 2, 1);
        assert_eq!(bond.accrued(&ref_date, true), 31.0 / 365.0 * 5.0);

        let bond = FixedBond {
            value_date: NaiveDate::from_ymd(2010, 1, 1),
            mty_date: NaiveDate::from_ymd(2012, 1, 1),
            redem_value: 100.0,
            cpn_rate: 0.05,
            cpn_freq: 0,
        };
        let ref_date = NaiveDate::from_ymd(2010, 2, 1);
        assert_eq!(
            bond.accrued(&ref_date, true),
            31.0 / (365.0 + 365.0) * (5.0 * 2.0)
        );
    }
    #[test]
    fn plain_bond() {
        let bond = FixedBond {
            value_date: NaiveDate::from_ymd(2010, 1, 1),
            mty_date: NaiveDate::from_ymd(2020, 1, 1),
            redem_value: 100.0,
            cpn_rate: 0.05,
            cpn_freq: 1,
        };
        let ytm = 0.05;
        let ref_date = NaiveDate::from_ymd(2010, 1, 1);
        assert_eq!(rnd(bond.result(&ref_date, 100.0).ytm), ytm);
        // won't change as the price is clean
        let ref_date = NaiveDate::from_ymd(2011, 1, 1);
        assert_eq!(rnd(bond.result(&ref_date, 100.0).ytm), ytm);
        // won't change as the price is clean
        let ref_date = NaiveDate::from_ymd(2011, 6, 15);
        assert_eq!(rnd(bond.result(&ref_date, 100.0).ytm), ytm);
    }
    #[test]
    fn zero_cpn_bond() {
        let bond = FixedBond {
            value_date: NaiveDate::from_ymd(2010, 1, 1),
            mty_date: NaiveDate::from_ymd(2011, 1, 1),
            redem_value: 100.0,
            cpn_rate: 0.05,
            cpn_freq: 0,
        };
        let ytm = 0.050000000000000114;
        let ref_date = NaiveDate::from_ymd(2010, 1, 1);
        assert_eq!(bond.result(&ref_date, 100.0).ytm, ytm);
    }
    #[test]
    fn cashflow() {
        let bond = FixedBond {
            value_date: NaiveDate::from_ymd(2010, 1, 1),
            mty_date: NaiveDate::from_ymd(2010, 8, 1),
            redem_value: 100.0,
            cpn_rate: 0.05,
            cpn_freq: 2,
        };
        let out = bond.cashflow().data;
        let mut expect: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        expect.insert(NaiveDate::from_ymd(2010, 7, 1), 2.5);
        expect.insert(NaiveDate::from_ymd(2010, 8, 1), 100.0 + 5.0 * 0.5 * 31.0 / 184.0);
        assert_eq!(out, expect);
    }
    #[test]
    fn dur() {
        let bond = FixedBond {
            value_date: NaiveDate::from_ymd(2010, 1, 1),
            mty_date: NaiveDate::from_ymd(2015, 1, 1),
            redem_value: 100.0,
            cpn_rate: 0.05,
            cpn_freq: 0,
        };
        let ref_date = NaiveDate::from_ymd(2010, 1, 1);
        assert_eq!(rnd2(bond.result(&ref_date, 100.0).macd), 5.0);
        let ref_date = NaiveDate::from_ymd(2011, 1, 1);
        assert_eq!(rnd2(bond.result(&ref_date, 100.0).macd), 4.0);
        let ref_date = NaiveDate::from_ymd(2010, 7, 1);
        assert_eq!(rnd2(bond.result(&ref_date, 100.0).macd), 4.5);

        let bond = FixedBond {
            value_date: NaiveDate::from_ymd(2010, 1, 1),
            mty_date: NaiveDate::from_ymd(2015, 1, 1),
            redem_value: 100.0,
            cpn_rate: 0.05,
            cpn_freq: 1,
        };
        let ref_date = NaiveDate::from_ymd(2010, 1, 1);
        let res = bond.result(&ref_date, 100.0);
        assert_eq!(rnd2(res.macd / (1.0 + res.ytm)), rnd2(res.modd));
    }
    #[test]
    fn add_months() {
        let ref_date = NaiveDate::from_ymd(2020, 12, 31);
        assert_eq!(FixedBond::add_months(&ref_date, 0), ref_date);
        assert_eq!(FixedBond::add_months(&ref_date, 1), NaiveDate::from_ymd(2021, 1, 31));
        assert_eq!(FixedBond::add_months(&ref_date, 2), NaiveDate::from_ymd(2021, 2, 28));
        assert_eq!(FixedBond::add_months(&ref_date, 11), NaiveDate::from_ymd(2021, 11, 30));
        assert_eq!(FixedBond::add_months(&ref_date, 12), NaiveDate::from_ymd(2021, 12, 31));
    }

    #[test]
    #[should_panic]
    fn panic_when_invalid_freq() {
        let bond = FixedBond {
            value_date: NaiveDate::from_ymd(2010, 1, 1),
            mty_date: NaiveDate::from_ymd(2011, 1, 1),
            redem_value: 100.0,
            cpn_rate: 0.05,
            cpn_freq: 3,
        };
        bond.cashflow();
    }
}
