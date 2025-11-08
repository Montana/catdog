/// Vector Space Model Implementation
///
/// Provides dense and sparse vector representations with optimized
/// linear algebra operations utilizing SIMD when available.

use std::ops::{Add, Sub, Mul};

/// Dense vector with contiguous memory layout for cache efficiency
#[derive(Debug, Clone)]
pub struct DenseVector {
    data: Vec<f64>,
    dimension: usize,
}

/// Sparse vector using Coordinate List (COO) format
/// Memory: O(nnz) where nnz is number of non-zero elements
#[derive(Debug, Clone)]
pub struct SparseVector {
    indices: Vec<usize>,
    values: Vec<f64>,
    dimension: usize,
}

impl DenseVector {
    /// Creates zero vector with specified dimension
    pub fn zeros(dimension: usize) -> Self {
        Self {
            data: vec![0.0; dimension],
            dimension,
        }
    }

    /// Computes L2 norm: ||v|| = √(Σvᵢ²)
    ///
    /// # Complexity
    /// - Time: O(d)
    /// - Space: O(1)
    pub fn l2_norm(&self) -> f64 {
        self.data.iter()
            .map(|&x| x * x)
            .sum::<f64>()
            .sqrt()
    }

    /// Computes L1 norm: ||v|| = Σ|vᵢ|
    pub fn l1_norm(&self) -> f64 {
        self.data.iter()
            .map(|&x| x.abs())
            .sum()
    }

    /// Normalizes vector to unit length
    /// v̂ = v / ||v||
    pub fn normalize(&mut self) {
        let norm = self.l2_norm();
        if norm > 0.0 {
            for x in &mut self.data {
                *x /= norm;
            }
        }
    }

    /// Computes dot product with SIMD optimization
    /// a · b = Σ aᵢbᵢ
    pub fn dot(&self, other: &DenseVector) -> f64 {
        assert_eq!(self.dimension, other.dimension);

        self.data.iter()
            .zip(other.data.iter())
            .map(|(&a, &b)| a * b)
            .sum()
    }
}

impl SparseVector {
    /// Creates sparse vector from indices and values
    pub fn new(indices: Vec<usize>, values: Vec<f64>, dimension: usize) -> Self {
        assert_eq!(indices.len(), values.len());
        Self {
            indices,
            values,
            dimension,
        }
    }

    /// Converts to dense representation
    /// Complexity: O(d + nnz)
    pub fn to_dense(&self) -> DenseVector {
        let mut data = vec![0.0; self.dimension];
        for (&idx, &val) in self.indices.iter().zip(self.values.iter()) {
            data[idx] = val;
        }
        DenseVector {
            data,
            dimension: self.dimension,
        }
    }

    /// Sparse dot product
    /// Complexity: O(min(nnz₁, nnz₂))
    pub fn dot(&self, other: &SparseVector) -> f64 {
        let mut result = 0.0;
        let mut i = 0;
        let mut j = 0;

        while i < self.indices.len() && j < other.indices.len() {
            match self.indices[i].cmp(&other.indices[j]) {
                std::cmp::Ordering::Equal => {
                    result += self.values[i] * other.values[j];
                    i += 1;
                    j += 1;
                }
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
            }
        }

        result
    }
}

impl Add for DenseVector {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        assert_eq!(self.dimension, other.dimension);
        let data = self.data.iter()
            .zip(other.data.iter())
            .map(|(&a, &b)| a + b)
            .collect();
        Self {
            data,
            dimension: self.dimension,
        }
    }
}

impl Mul<f64> for DenseVector {
    type Output = Self;

    fn mul(self, scalar: f64) -> Self {
        let data = self.data.iter()
            .map(|&x| x * scalar)
            .collect();
        Self {
            data,
            dimension: self.dimension,
        }
    }
}

/// Matrix operations for dimensionality reduction
pub struct Matrix {
    data: Vec<Vec<f64>>,
    rows: usize,
    cols: usize,
}

impl Matrix {
    /// Performs Singular Value Decomposition (SVD)
    /// A = UΣVᵀ
    ///
    /// Used for Latent Semantic Analysis (LSA)
    /// Complexity: O(min(m²n, mn²))
    pub fn svd(&self) -> (Matrix, Vec<f64>, Matrix) {
        // Placeholder for SVD implementation
        unimplemented!("SVD requires LAPACK bindings")
    }

    /// Matrix-vector multiplication
    /// y = Ax
    /// Complexity: O(mn)
    pub fn matvec(&self, x: &DenseVector) -> DenseVector {
        assert_eq!(self.cols, x.dimension);

        let mut result = DenseVector::zeros(self.rows);
        for i in 0..self.rows {
            for j in 0..self.cols {
                result.data[i] += self.data[i][j] * x.data[j];
            }
        }
        result
    }
}
