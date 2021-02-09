use std::cmp::Ordering;
use std::str::FromStr;

use bigdecimal::{BigDecimal, One, Signed, ToPrimitive, Zero};
use num_bigint::BigInt;

// ipow' :: Num a => a -> Integer -> a
// ipow' x n
//   | n == 0 = 1
//   | m == 0 = let y = ipow' x d in y * y
//   | otherwise = x * ipow' x (n - 1)
//   where (d,m) = divMod n 2
fn ipow_p(x: &BigDecimal, n: i32) -> BigDecimal {
    if n == 0 {
        return BigDecimal::one();
    }
    let d = n / 2_i32;
    let m = n % 2_i32;
    if m == 0 {
        return normalize(ipow_p(x, d).square());
    }

    normalize(x * ipow_p(x, n - 1))
}

// ipow :: Fractional a => a -> Integer -> a
// ipow x n
//   | n < 0 = 1 / ipow' x (-n)
//   | otherwise = ipow' x n
pub fn ipow(x: &BigDecimal, n: i32) -> BigDecimal {
    if n < 0 {
        normalize(ipow_p(x, -n).inverse())
    } else {
        ipow_p(x, n)
    }
}

// logAs :: (Num a) => a -> [a]
// logAs a = a' : a' : logAs (a + 1)
//   where
//     a' = a * a
// fn log_as(a: &BigDecimal) -> &[BigDecimal] {
//
// }

// -- | Approximate ln(1+x) for x \in [0, \infty)
// -- a_1 = x, a_{2k} = a_{2k+1} = x·k^2, k >= 1
// -- b_n = n, n >= 0
// lncf :: (Fractional a, Enum a, Ord a, Show a) => Int -> a -> a
// lncf maxN x
//   | x < 0     = error ("x = " ++ show x ++ " is not inside domain [0,..)")
//   | otherwise = cf maxN 0 eps Nothing 1 0 0 1 as [1,2..]
//   where as = x : map (*x) (logAs 1)
fn lncf(max_n: i32, x: &BigDecimal) -> BigDecimal {
    if x < &BigDecimal::zero() {
        panic!("Cannot call lncf with x < 0")
    }

    let eps = BigDecimal::from_str("1.E-24").unwrap();
    cf(
        max_n,
        x,
        &eps,
        &BigDecimal::one(),
        &BigDecimal::zero(),
        &BigDecimal::zero(),
        &BigDecimal::one(),
    )
}

// -- | Compute natural logarithm via continued fraction, first splitting integral
// -- part and then using continued fractions approximation for `ln(1+x)`
// ln' :: (RealFrac a, Enum a, Show a) => a -> a
// ln' x
//   | x <= 0    = error (show x ++ " is not in domain of ln")
//   | otherwise = fromIntegral n + lncf 1000 x'
//   where (n, x') = splitLn x
pub fn ln(x: &BigDecimal) -> BigDecimal {
    if x <= &BigDecimal::zero() {
        panic!("X must be positive, non zero");
    }

    let exp1 = exp(&BigDecimal::one());
    let (n, xp) = split_ln(&exp1, x);

    BigDecimal::from(n) + lncf(1000, &xp)
}

// -- | Compute continued fraction using max steps or bounded list of a/b factors.
// -- The 'maxN' parameter gives the maximum recursion depth, 'n' gives the current
// -- rursion depth, 'lastVal' is the optional last value ('Nothing' for the first
// -- iteration). 'aNm2' / 'bNm2' are A_{n-2} / B_{n-2}, 'aNm1' / 'bNm1' are
// -- A_{n-1} / B_{n-1}, and 'aN' / 'bN' are A_n / B_n respectively, 'an' / 'bn'
// -- are lists of succecsive a_n / b_n values for the recurrence relation:
// --
// -- A_{-1} = 1,    A_0 = b_0
// -- B_{-1} = 0,    B_0 = 1
// -- A_n = b_n*A_{n-1} + a_n*A_{n-2}
// -- B_n = b_n*B_{n-1} + a_n*B_{n-2}
// --
// -- The convergent 'xn' is calculated as x_n = A_n/B_n
// --
// --                        a_1
// -- result = b_0 + ---------------------
// --                           a_2
// --                b_1 + ---------------
// --                              a_3
// --                      b_2 + ---------
// --                                  .
// --                            b_3 +  .
// --                                    .
// --
// -- The recursion stops once 'maxN' iterations have been reached, or either the
// -- list 'as' or 'bs' is exhausted or 'lastVal' differs less than 'epsilon' from the
// -- new convergent.
// cf ::
//      (Fractional a, Ord a, Show a)
//   => Int
//   -> Int
//   -> a
//   -> Maybe a
//   -> a
//   -> a
//   -> a
//   -> a
//   -> [a]
//   -> [a]
//   -> a
// cf maxN n epsilon lastVal aNm2 bNm2 aNm1 bNm1 (an:as) (bn:bs)
//   | maxN == n = xn
//   | converges = xn
//   | otherwise = cf maxN (n + 1) epsilon (Just xn) aNm1 bNm1 aN bN as bs
//   where
//     converges = maybe False (\x -> abs (x - xn) < epsilon) lastVal
//     xn = aN / bN -- convergent
//     aN = bn * aNm1 + an * aNm2
//     bN = bn * bNm1 + an * bNm2
// cf _ _ _ _ _ _ aN bN _ _ = aN / bN
//fn cf(max_n: i32, n: i32, epsilon: &BigDecimal, last_val: Option<&BigDecimal>, a_nm2: &BigDecimal, b_nm2: &BigDecimal, a_nm1: &BigDecimal, b_nm1: &BigDecimal, mut range: RangeFrom<i32>) -> BigDecimal {
// let b_bn = normalize(BigDecimal::from(range.by_ref().take(1).next().unwrap()));
// let a_an = normalize(&b_bn * &b_bn);
// let a_n = normalize(&b_bn * a_nm1 + &a_an * a_nm2);
// let b_n = normalize(&b_bn * b_nm1 + &a_an * b_nm2);
// let xn = normalize(&a_n / &b_n);
// if max_n == n {
//     return xn;
// }
// let converges = match last_val {
//     None => { false }
//     Some(x) => { &(x - &xn).abs() < epsilon }
// };
// if converges {
//     return xn;
// }
//
// return cf(max_n, n + 1, epsilon, Some(&xn), a_nm1, b_nm1, &a_n, &b_n, range);
//}

// def cf(maxN,x,epsilon,aNm2,bNm2,aNm1,bNm1):
//     an = x
//     bn = 1
//     aN = bn * aNm1 + an * aNm2
//     bN = bn * bNm1 + an * bNm2
//     aNm2 = aNm1
//     bNm2 = bNm1
//     aNm1 = aN
//     bNm1 = bN
//     x_ = aN / bN
//     for n in range(2,maxN+1):
//         if n % 2 == 0:
//             an = (n/2)**2 * x
//         bn = n
//         aN = bn * aNm1 + an * aNm2
//         bN = bn * bNm1 + an * bNm2
//         aNm2 = aNm1
//         bNm2 = bNm1
//         aNm1 = aN
//         bNm1 = bN
//         xn = aN / bN
//         if abs(x_ - xn) < epsilon:
//             return xn
//         x_ = xn
//     return x_
fn cf(
    max_n: i32,
    x: &BigDecimal,
    epsilon: &BigDecimal,
    a_nm_2: &BigDecimal,
    b_nm_2: &BigDecimal,
    a_nm_1: &BigDecimal,
    b_nm_1: &BigDecimal,
) -> BigDecimal {
    let mut an = x.clone();
    let mut bn = BigDecimal::one();
    let mut a_n = normalize(&bn * a_nm_1 + &an * a_nm_2);
    let mut b_n = normalize(&bn * b_nm_1 + &an * b_nm_2);
    let mut a_nm_2 = a_nm_1.clone();
    let mut b_nm_2 = b_nm_1.clone();
    let mut a_nm_1 = a_n.clone();
    let mut b_nm_1 = b_n.clone();
    let mut xp = normalize(&a_n / &b_n);
    for n in 2..(max_n + 1) {
        if n % 2 == 0 {
            an = normalize(BigDecimal::from((n / 2).pow(2)) * x);
        }
        bn = BigDecimal::from(n);
        a_n = normalize(&bn * &a_nm_1 + &an * &a_nm_2);
        b_n = normalize(&bn * &b_nm_1 + &an * &b_nm_2);
        a_nm_2 = a_nm_1.clone();
        b_nm_2 = b_nm_1.clone();
        a_nm_1 = a_n.clone();
        b_nm_1 = b_n.clone();
        let xn = normalize(&a_n / &b_n);
        if (&xp - &xn).abs() < *epsilon {
            return xn;
        }
        xp = xn;
    }

    xp
}

// taylorExp :: (RealFrac a, Show a) => Int -> Int -> a -> a -> a -> a -> a
// taylorExp maxN n x lastX acc divisor
// | maxN == n = acc
// | abs nextX < eps = acc
// | otherwise = taylorExp maxN (n + 1) x nextX (acc + nextX) (divisor + 1)
// where nextX = (lastX * x) / divisor
fn taylor_exp(
    eps: &BigDecimal,
    max_n: i32,
    n: i32,
    x: &BigDecimal,
    last_x: &BigDecimal,
    acc: &BigDecimal,
    divisor: &BigDecimal,
) -> BigDecimal {
    if max_n == n {
        return acc.clone();
    }
    let next_x = normalize((last_x * x) / divisor);
    if &next_x.abs() < eps {
        return acc.clone();
    }

    taylor_exp(
        eps,
        max_n,
        n + 1,
        x,
        &next_x,
        &(acc + &next_x),
        &(divisor + BigDecimal::one()),
    )
}

pub enum TaylorCmp {
    Above,
    Below,
    MaxReached,
}

pub fn taylor_exp_cmp(bound_x: i32, cmp: &BigDecimal, x: &BigDecimal) -> TaylorCmp {
    let max_n: i32 = 1000;
    let bound_xf = BigDecimal::from(bound_x);
    let mut divisor: i32 = 1;
    let mut acc: BigDecimal = BigDecimal::one();
    let mut err: BigDecimal = x.clone();
    let mut error_term: BigDecimal = normalize(&err * &bound_xf);
    let mut next_x: BigDecimal;
    for _n in 0..max_n {
        if cmp >= &normalize(&acc + &error_term) {
            return TaylorCmp::Above;
        } else if cmp < &normalize(&acc - &error_term) {
            return TaylorCmp::Below;
        } else {
            divisor += 1;
            next_x = err.clone();
            err = normalize(normalize(&err * x) / BigDecimal::from(divisor));
            error_term = normalize(&err * &bound_xf);
            acc = normalize(&acc + &next_x);
        }
    }

    TaylorCmp::MaxReached
}

// splitLn :: (RealFrac a, Show a) => a -> (Integer, a)
// splitLn x = (n, x')
//   where n = findE exp1 x
//         y' = ipow exp1 n
//         x' = (x / y') - 1 -- x / e^n > 1!
pub fn split_ln(exp1: &BigDecimal, x: &BigDecimal) -> (i32, BigDecimal) {
    let n = find_e(exp1, x);
    let yp = ipow(exp1, n);
    let xp = (x / yp) - BigDecimal::one();

    (n, normalize(xp))
}

pub fn ceiling(x: &BigDecimal) -> BigDecimal {
    if x.is_integer() {
        x.with_scale(0)
    } else {
        (x + BigDecimal::one()).with_scale(0)
    }
}

// scaleExp :: (RealFrac a) => a -> (Integer, a)
// scaleExp x = (x', x / fromIntegral x')
//   where x' = ceiling x
fn scale_exp(x: &BigDecimal) -> (i32, BigDecimal) {
    let xp = ceiling(x);
    (xp.to_i32().unwrap(), normalize(x / xp))
}

// exp' :: (RealFrac a, Show a) => a -> a
// exp' x
//   | x < 0     = 1 / exp' (-x)
//   | otherwise = ipow x' n
//   where (n, x_) = scaleExp x
//         x'      = taylorExp 1000 1 x_ 1 1 1
pub fn exp(x: &BigDecimal) -> BigDecimal {
    let zero = BigDecimal::zero();
    match x.cmp(&zero) {
        Ordering::Equal => BigDecimal::one(),
        Ordering::Less => normalize(exp(&-x).inverse()),
        Ordering::Greater => {
            let (n, x_) = scale_exp(x);
            let eps = BigDecimal::from_str("1.E-24").unwrap();
            let xp = taylor_exp(
                &eps,
                1000,
                1,
                &x_,
                &BigDecimal::one(),
                &BigDecimal::one(),
                &BigDecimal::one(),
            );
            ipow(&xp, n)
        }
    }
}

// -- | find n with `e^n<=x<e^(n+1)`
// findE :: (RealFrac a) => a -> a -> Integer
// findE e x = contract e x lower upper
//   where
//     (lower, upper) = bound e x (1/e) e (-1) 1
pub fn find_e(e: &BigDecimal, x: &BigDecimal) -> i32 {
    let (lower, upper) = bound(e, x, &e.inverse(), e, -1, 1);
    contract(e, x, lower, upper)
}

// -- | Simple way to find integer powers that bound x. At every step the bounds
// -- are doubled. Assumption x > 0, the calculated bound is `factor^l <= x <=
// -- factor^u`, initially x' is assumed to be `1/factor` and x'' `factor`, l = -1
// -- and u = 1.
// bound ::
//      (Fractional a, Ord a)
//   => a
//   -> a
//   -> a
//   -> a
//   -> Integer
//   -> Integer
//   -> (Integer, Integer)
// bound factor x x' x'' l u
//   | x' <= x && x <= x'' = (l, u)
//   | otherwise = bound factor x (x' * x') (x'' * x'') (2 * l) (2 * u)
fn bound(factor: &BigDecimal, x: &BigDecimal, xp: &BigDecimal, xpp: &BigDecimal, l: i32, u: i32) -> (i32, i32) {
    if xp <= x && x <= xpp {
        (l, u)
    } else {
        bound(factor, x, &xp.square(), &xpp.square(), 2 * l, 2 * u)
    }
}

// -- | Bisect bounds to find the smallest integer power such that
// -- `factor^n<=x<factor^(n+1)`.
// contract ::
//      (Fractional a, Ord a)
//   => a
//   -> a
//   -> Integer
//   -> Integer
//   -> Integer
// contract factor x = go
//   where
//     go l u
//       | l + 1 == u = l
//       | otherwise =
//         if x < x'
//           then go l mid
//           else go mid u
//       where
//         mid = l + ((u - l) `div` 2)
//         x' = ipow factor mid
fn contract(factor: &BigDecimal, x: &BigDecimal, l: i32, u: i32) -> i32 {
    if l + 1 == u {
        l
    } else {
        let mid = l + ((u - l) / 2);
        let xp = ipow(factor, mid);
        if x < &xp {
            contract(factor, x, l, mid)
        } else {
            contract(factor, x, mid, u)
        }
    }
}

pub fn normalize(x: BigDecimal) -> BigDecimal {
    x.with_scale(34)
}

pub fn round(x: BigDecimal) -> BigDecimal {
    let round_digits = 34_i64;
    let (bigint, decimal_part_digits) = &x.as_bigint_and_exponent();
    let need_to_round_digits = decimal_part_digits - round_digits;
    if round_digits >= 0 && need_to_round_digits <= 0 {
        return x;
    }

    let mut number = bigint.clone();
    if number < BigInt::from(0i32) {
        number = -number;
    }
    for _ in 0..(need_to_round_digits - 1) {
        number /= 10;
    }
    let digit = number % 10;

    if digit <= BigInt::from(4i32) {
        x.with_scale(round_digits)
    } else if bigint.is_negative() {
        x.with_scale(round_digits) - BigDecimal::new(BigInt::from(1), round_digits)
    } else {
        x.with_scale(round_digits) + BigDecimal::new(BigInt::from(1), round_digits)
    }
}
