use crate::matrix::Matrix;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufReader, Read};

#[derive(Serialize, Deserialize, Debug)]
pub struct ModelData {
    pub norm: FieldsDescribe,
    pub weights: Weights,
    pub fields: Vec<String>,
    pub alpha: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Weights {
    #[serde(rename = "dense/bias:0")]
    pub l0_bias: Vec<f32>,
    #[serde(rename = "dense/kernel:0")]
    pub l0_kernel: Matrix,

    #[serde(rename = "dense_1/bias:0")]
    pub l1_bias: Vec<f32>,
    #[serde(rename = "dense_1/kernel:0")]
    pub l1_kernel: Matrix,

    #[serde(rename = "dense_2/bias:0")]
    pub l2_bias: Vec<f32>,
    #[serde(rename = "dense_2/kernel:0")]
    pub l2_kernel: Matrix,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FieldsDescribe {
    mean: HashMap<String, f32>,
    std: HashMap<String, f32>,
}

impl ModelData {
    pub fn from_reader<R: Read>(reader: R) -> Result<Self, Error> {
        let buffer = BufReader::new(reader);
        let model: Self = serde_cbor::from_reader(buffer)?;
        Ok(model)
    }

    pub fn predict(&self, input: &Matrix) -> Result<f32, Error> {
        let a1 = input.dot(&self.weights.l0_kernel)?;
        let a2 = a1.add(&self.weights.l0_bias)?;
        let a3 = a2.relu(self.alpha);

        let b1 = a3.dot(&self.weights.l1_kernel)?;
        let b2 = b1.add(&self.weights.l1_bias)?;
        let b3 = b2.relu(self.alpha);

        let c1 = b3.dot(&self.weights.l2_kernel)?;
        let c2 = c1.add(&self.weights.l2_bias)?;

        Ok(c2[0][0])
    }

    pub fn norm(&self, input: &HashMap<String, f32>) -> Result<Matrix, Error> {
        let mut result = vec![];
        for field in self.fields.iter() {
            let x = input.get(field).unwrap_or(&0.0);
            let std = self
                .norm
                .std
                .get(field)
                .ok_or_else(|| Error::MissingStdData(field.clone()))?;
            let mean = self
                .norm
                .mean
                .get(field)
                .ok_or_else(|| Error::MissingMeanData(field.clone()))?;
            let res = (x - mean) / std;
            result.push(res)
        }
        Ok(Matrix::from_array(result))
    }

    pub fn norm_predict(&self, input: &HashMap<String, f32>) -> Result<f32, Error> {
        let input = self.norm(input)?;
        self.predict(&input)
    }
}

#[cfg(test)]
pub mod tests {
    use crate::matrix::Matrix;
    use crate::ModelData;
    use float_cmp::{ApproxEq, F32Margin};
    use std::collections::HashMap;
    use std::io::Cursor;

    pub const MARGIN: F32Margin = F32Margin {
        ulps: 2,
        epsilon: 0.0001,
    };

    pub fn get_test_model() -> ModelData {
        let model_bytes = include_bytes!("../models/test_model.cbor");
        assert_eq!(1799, model_bytes.len(), "test model bytes not expected");
        let model = ModelData::from_reader(Cursor::new(model_bytes));
        assert!(model.is_ok(), "can't restore model from bytes");
        model.unwrap()
    }

    #[rustfmt::skip]
    pub fn get_test_input() -> Matrix {
        Matrix::from_array(vec![-0.23901028,  0.02662498, -0.19410163,  0.03187769, -0.2026636,  -0.31123071,
                                     -0.23820148,  4.48084238,  0.86297716, -0.00825855, -0.1420311,  -0.5924509,
                                      0.62382793, -0.77146702, -0.5813809,  -0.36034099,  0.88637573,  0.3041703,
                                      0.6286678,  -1.48856029])
    }

    pub const BUCKETS: [u64; 16] = [
        13u64, 1, 32, 24, 14, 62, 1174, 453, 197, 291, 333, 3304, 307, 229, 36, 58,
    ];

    pub fn get_test_pre_norm() -> HashMap<String, f32> {
        let mut map = HashMap::new();
        map.insert("confirms_in".to_string(), 11.0);
        for (i, el) in BUCKETS.iter().enumerate() {
            map.insert(format!("b{}", i), *el as f32);
        }
        map.insert("delta_last".to_string(), 956.0);
        map.insert("day_of_week".to_string(), 4.0);
        map.insert("hour".to_string(), 4.0);
        map
    }

    pub fn get_test_result() -> f32 {
        25.89434588
    }

    #[test]
    fn test_predict() {
        let model = get_test_model();
        let input = get_test_input();
        assert!(get_test_result().approx_eq(model.predict(&input).unwrap(), MARGIN))
    }

    #[test]
    fn test_vector() {
        let model = get_test_model();
        let input = get_test_input();
        assert_eq!((1, 20), input.size());

        let a1 = input.dot(&model.weights.l0_kernel).unwrap();
        let a1_expected = Matrix::from_array(vec![-8.07738634, 0.32887421, 2.60496564, 0.14431801]);
        assert!(a1.approx_eq(&a1_expected));
        let a2 = a1.add(&model.weights.l0_bias).unwrap();
        let a2_expected =
            Matrix::from_array(vec![-9.79705103, 1.19654123, 2.06540848, -0.23819596]);
        assert!(a2.approx_eq(&a2_expected));
        let a3 = a2.relu(0.01);

        let b1 = a3.dot(&model.weights.l1_kernel).unwrap();
        let b2 = b1.add(&model.weights.l1_bias).unwrap();
        let b3 = b2.relu(0.01);
        let b3_expected = Matrix::from_array(vec![-0.00769195, 4.21514198, 5.28356369, 5.090146]);
        assert!(b3.approx_eq(&b3_expected));

        let c1 = b3.dot(&model.weights.l2_kernel).unwrap();
        let c2 = c1.add(&model.weights.l2_bias).unwrap();

        assert!(get_test_result().approx_eq(c2[0][0], MARGIN))
    }

    #[test]
    #[rustfmt::skip]
    fn test_norm() {
        let model = get_test_model();
        let expected = get_test_input();
        let norm = model.norm(&get_test_pre_norm()).unwrap();
        assert!(norm.approx_eq(&expected), "normalization is wrong");
    }
}
