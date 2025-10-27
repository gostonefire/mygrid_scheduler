use crate::errors::SplineError;

/// Struct for making a Monotonic Cube Spline interpolation
/// 
pub struct MonotonicCubicSpline {
    m_x: Vec<f64>,
    m_y: Vec<f64>,
    m_m: Vec<f64>
}

impl MonotonicCubicSpline {
    /// Returns a new instance with prepared slopes
    /// 
    /// # Argument
    /// * 'x' - vector of x:es
    /// * 'y' - vector of y:s
    pub fn new(x : &Vec<f64>, y : &Vec<f64>) -> Result<MonotonicCubicSpline, SplineError> {
        
        if x.len() != y.len() || x.len() < 2 || y.len() < 2 {
            return Err(SplineError("x is too short".to_string()));
        }

        let n = x.len();

        let mut secants = vec![0.0 ; n - 1];
        let mut slopes  = vec![0.0 ; n];

        for i in 0..(n-1) {
            let h = *x.get(i + 1).unwrap() - *x.get(i).unwrap();
            if h <= 0.0 {
                return Err(SplineError("control points not monotonically increasing".to_string()));
            }
            secants[i] = (*y.get(i + 1).unwrap() - *y.get(i).unwrap()) / h;

        }

        slopes[0] = secants[0];
        for i in 1..(n-1) {
            slopes[i] = (secants[i - 1] + secants[i]) * 0.5;
        }
        slopes[n - 1] = secants[n - 2];

        for i in 0..(n-1) {
            if secants[i] == 0.0 {
                slopes[i] = 0.0;
                slopes[i + 1] = 0.0;
            } else {
                let alpha = slopes[i] / secants[i];
                let beta = slopes[i + 1] / secants[i];
                let h = alpha.hypot(beta);
                if h > 9.0 {
                    let t = 3.0 / h;
                    slopes[i] = t * alpha * secants[i];
                    slopes[i + 1] = t * beta * secants[i];
                }
            }
        }

        let spline = MonotonicCubicSpline {
            m_x: x.clone(),
            m_y: y.clone(),
            m_m: slopes
        };

        Ok(spline)
    }

    fn hermite(point: f64, x : (f64, f64), y: (f64, f64), m: (f64, f64)) -> f64 {

        let h: f64 = x.1 - x.0;
        let t = (point - x.0) / h;
        (y.0 * (1.0 + 2.0 * t) + h * m.0 * t) * (1.0 - t) * (1.0 - t)
            + (y.1 * (3.0 - 2.0 * t) + h * m.1 * (t - 1.0)) * t * t
    }

    /// Interpolates a y for the given point
    /// 
    /// # Arguments
    /// 
    /// * 'point' - x to get an interpolated y for
    pub fn interpolate(&self, point : f64) -> f64 {
       
        let n = self.m_x.len();

        if point <= *self.m_x.get(0).unwrap() {
            return *self.m_y.get(0).unwrap();
        }
        if point >= *self.m_x.get(n - 1).unwrap() {
            return *self.m_y.get(n - 1).unwrap();
        }

        let mut i = 0;
        while point >= *self.m_x.get(i + 1).unwrap() {
            i += 1;
            if point == *self.m_x.get(i).unwrap() {
                return *self.m_y.get(i).unwrap();
            }
        }
        MonotonicCubicSpline::hermite(
            point,
            (*self.m_x.get(i).unwrap(), *self.m_x.get(i+1).unwrap()),
            (*self.m_y.get(i).unwrap(), *self.m_y.get(i+1).unwrap()),
            (*self.m_m.get(i).unwrap(), *self.m_m.get(i+1).unwrap())
        )
    }
}
