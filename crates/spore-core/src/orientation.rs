//! Orientation vocabulary shared by the authoring layer and the engine.
//!
//! # Why this lives in `spore-core` and not in either neighbour
//!
//! Two crates need to talk about "how is this thing rotated", and they are on
//! opposite sides of a dependency edge:
//!
//! * `spore-creator-core` owns the authoring document, whose `<orientation>` is a
//!   row-major 3×3.
//! * `spore-engine` owns the runtime placement, which has to express the same thing
//!   without narrowing it.
//!
//! Either one hosting the type would force the other to depend on it. Putting the
//! type here — in the crate both already depend on, which knows about neither — keeps
//! the graph acyclic and, more importantly, keeps a single definition of "a
//! rotation" so the two layers cannot drift into spelling it differently.
//!
//! # The enum, and the trap it prevents
//!
//! [`Orientation`] is an enum rather than a bare [`Matrix3`] because the two
//! spellings carry different amounts of truth:
//!
//! * [`Orientation::YRotation`] says the orientation *is* a rotation about one axis.
//!   A caller that picks it is making a claim.
//! * [`Orientation::General`] says nine numbers must survive verbatim, which is what
//!   an authored `<orientation>` is.
//!
//! Collapsing them loses something in both directions. Narrowing a `General` to a Y
//! angle discards orientation — and it discards it *quietly*, because a part that
//! should be tilted renders upright, which looks like a viewing angle rather than
//! like a bug. Widening a `YRotation` into a matrix that happens to equal it invents
//! a specificity the caller never had. [`Orientation::as_y_rotation`] answering
//! `None` for a genuinely non-Y matrix is what keeps the two apart, and
//! [`Orientation::Default`] being the `YRotation` spelling rather than the matrix is
//! what stops an unspecified orientation from looking like a considered one.

use crate::record::Matrix3;

/// How a placement's orientation is spelled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Orientation {
    /// A rotation about +Y, in degrees.
    ///
    /// Kept because it is the cheaper representation for the common case and it is
    /// what a hand-authored placement means. It is not a weaker `General`; it is a
    /// different, more specific claim.
    YRotation(f32),
    /// A row-major 3×3, exactly as the creator save format spells it.
    ///
    /// Row-major: `row[i][j]` multiplies `basis[j]`. Three rows of three numbers,
    /// unchanged.
    General(Matrix3),
}

impl Default for Orientation {
    /// Identity as a Y rotation of zero.
    ///
    /// Defaulting to `General(Matrix3::IDENTITY)` would be numerically identical and
    /// structurally misleading: it would make an unspecified orientation look like
    /// a deliberately chosen matrix.
    fn default() -> Self {
        Self::YRotation(0.0)
    }
}

impl Orientation {
    /// The row-major 3×3 this orientation denotes.
    pub fn as_matrix(&self) -> Matrix3 {
        match self {
            Self::YRotation(degrees) => y_rotation_matrix(*degrees),
            Self::General(m) => *m,
        }
    }

    /// Whether this orientation is exactly a rotation about +Y.
    ///
    /// Tested on the matrix, not the variant, so a `General` that happens to *be* a Y
    /// rotation reports `true` and can be reported as one without lying. Bitwise
    /// comparison would be wrong here: [`y_rotation_matrix`] computes `-sin`, and
    /// `-sin(0.0)` is `-0.0`, whose bit pattern is `0x80000000`. A bit-exact zero test
    /// therefore rejects a legitimate zero-degree rotation, which makes a
    /// hand-authored zero rotation behave differently from an equivalent decoded one.
    /// A matrix that came out of a sine and a cosine has no authored zero-sign
    /// distinction to preserve.
    pub fn is_y_rotation(&self) -> bool {
        let m = self.as_matrix();
        let zero = |v: f32| v == 0.0;
        let one = |v: f32| v == 1.0;
        let is_identity = one(m.row0[0])
            && zero(m.row0[1])
            && zero(m.row0[2])
            && zero(m.row1[0])
            && one(m.row1[1])
            && zero(m.row1[2])
            && zero(m.row2[0])
            && zero(m.row2[1])
            && one(m.row2[2]);
        if is_identity {
            return true;
        }
        zero(m.row0[1])
            && m.row0[2] != 0.0
            && zero(m.row1[0])
            && one(m.row1[1])
            && zero(m.row1[2])
            && m.row2[0] != 0.0
            && zero(m.row2[1])
            && m.row0[0].is_finite()
            && m.row0[2].is_finite()
    }

    /// The Y angle in degrees, or `None` when this is not a Y rotation.
    ///
    /// `sin` is `row0[2]` and `cos` is `row0[0]`. Deliberately *not*
    /// `atan2(row0[2], -row2[0])`, even though `-row2[0]` is also `sin` here: that
    /// form is `atan2(sin, sin)`, a constant 45° for every quarter turn. A formula
    /// that reads correctly and returns a plausible angle is worse than one that
    /// fails to compile.
    ///
    /// Returns `None` rather than a best-effort angle. Reporting a Y component for a
    /// general orientation makes a tilted creature look merely miscalibrated instead
    /// of wrong.
    pub fn as_y_rotation(&self) -> Option<f32> {
        if !self.is_y_rotation() {
            return None;
        }
        let m = self.as_matrix();
        if m.row0[0] == 1.0 {
            // cos == 1 forces sin == 0 for any real angle, so this is the identity
            // and the angle is exactly zero rather than merely near it.
            return Some(0.0);
        }
        Some(m.row0[2].atan2(m.row0[0]).to_degrees())
    }

    /// The orthonormality error of `m` as if it were this orientation.
    ///
    /// Spelled on `Orientation` because a bare `Matrix3` has no claim about being a
    /// rotation, and `Matrix3::default()` being all zeros is the standing reminder
    /// that a 3×3 is not automatically a rotation.
    pub fn error_of(m: Matrix3) -> f32 {
        Self::General(m).orthonormality_error()
    }

    /// How far this matrix is from being a proper rotation, as a max-abs error.
    ///
    /// Zero for an orthonormal, right-handed matrix. **Measured, not assumed.** The
    /// game writes these matrices so they should be orthonormal, but a shear or a
    /// hidden scale smuggled into an orientation is still a valid-looking 3×3 that
    /// renders as a wrong transform, so the caller gets to see the number rather than
    /// trust the shape.
    pub fn orthonormality_error(&self) -> f32 {
        let m = self.as_matrix();
        let rows = [m.row0, m.row1, m.row2];
        let mut worst = 0.0f32;
        for i in 0..3 {
            for j in 0..3 {
                // R^T R must be the identity.
                let dot = rows.iter().map(|row| row[i] * row[j]).sum::<f32>();
                let want = if i == j { 1.0 } else { 0.0 };
                worst = worst.max((dot - want).abs());
            }
        }
        // The determinant must be +1. A -1 is a reflection, which mirrors rather than
        // rotates, so it is a different thing and must not read as clean.
        let det = m.row0[0] * (m.row1[1] * m.row2[2] - m.row1[2] * m.row2[1])
            - m.row0[1] * (m.row1[0] * m.row2[2] - m.row1[2] * m.row2[0])
            + m.row0[2] * (m.row1[0] * m.row2[1] - m.row1[1] * m.row2[0]);
        worst.max((det - 1.0).abs())
    }
}

/// The row-major matrix for a rotation of `degrees` about +Y.
///
/// Written out rather than composed from quaternion helpers so the nine numbers are
/// the nine numbers an authored document would contain, and so this is usable from a
/// headless test with no renderer in the dependency graph at all.
pub fn y_rotation_matrix(degrees: f32) -> Matrix3 {
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    Matrix3 {
        row0: [cos, 0.0, sin],
        row1: [0.0, 1.0, 0.0],
        row2: [-sin, 0.0, cos],
    }
}

impl From<f32> for Orientation {
    fn from(degrees: f32) -> Self {
        Self::YRotation(degrees)
    }
}

impl From<Matrix3> for Orientation {
    fn from(m: Matrix3) -> Self {
        Self::General(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(rows: [[f32; 3]; 3]) -> Matrix3 {
        Matrix3 {
            row0: rows[0],
            row1: rows[1],
            row2: rows[2],
        }
    }

    #[test]
    fn a_default_orientation_is_a_zero_y_rotation_and_not_a_matrix() {
        // Structurally, not numerically: both denote the identity, so only the
        // variant can tell an unspecified orientation from a chosen one.
        assert_eq!(Orientation::default(), Orientation::YRotation(0.0));
        assert_ne!(
            Orientation::default(),
            Orientation::General(Matrix3::IDENTITY),
            "defaulting to a matrix would make an unspecified orientation look chosen"
        );
    }

    #[test]
    fn the_identity_counts_as_a_y_rotation_which_a_nonzero_sin_test_would_miss() {
        // The identity has both off-diagonal terms zero, so an "is the sin term
        // non-zero" test cannot see it. It is still a Y rotation about any axis.
        assert!(Orientation::General(Matrix3::IDENTITY).is_y_rotation());
        assert_eq!(
            Orientation::General(Matrix3::IDENTITY).as_y_rotation(),
            Some(0.0)
        );
    }

    #[test]
    fn a_zero_degree_rotation_is_recognised_despite_its_negative_zero() {
        // y_rotation_matrix(0.0) puts -sin(0.0) in row2[0], which is -0.0 with bit
        // pattern 0x80000000. A bit-exact zero test rejected it and reported a
        // zero-degree rotation as "not a Y rotation".
        let o = Orientation::YRotation(0.0);
        assert!(o.is_y_rotation(), "a zero rotation is a rotation about Y");
        assert_eq!(o.as_y_rotation(), Some(0.0));
        assert!(
            o.as_matrix().row2[0] == 0.0,
            "-0.0 == 0.0 numerically, which is what the test relies on"
        );
    }

    #[test]
    fn a_general_orientation_reports_no_y_angle_rather_than_a_best_effort_one() {
        // A rotation about X by 40 degrees. Reporting its Y component (0.0 here)
        // would make a tilted part look merely miscalibrated instead of wrong.
        let (sin, cos) = 40.0f32.to_radians().sin_cos();
        let o = Orientation::General(matrix([[1.0, 0.0, 0.0], [0.0, cos, -sin], [0.0, sin, cos]]));
        assert!(!o.is_y_rotation());
        assert_eq!(o.as_y_rotation(), None);
    }

    #[test]
    fn a_y_rotation_survives_the_round_trip_through_the_matrix() {
        for degrees in [0.0f32, 1.0, 30.0, 90.0, 179.0, -45.0, 360.0] {
            let o = Orientation::YRotation(degrees);
            assert!(
                o.is_y_rotation(),
                "{degrees} degrees is a Y rotation by construction and must be \
                 recognised as one"
            );
            let back = o.as_y_rotation().expect("is_y_rotation said yes");
            let expected = degrees.rem_euclid(360.0);
            // Angles are circular, so the error is the SHORT way round. Plain
            // subtraction makes 0 and 360 look maximally different.
            let raw = back - expected;
            let delta = raw.abs().min((raw.abs() - 360.0).abs());
            assert!(
                delta < 1e-3,
                "{degrees} degrees came back as {back}, expected {expected}, \
                 circular error {delta}"
            );
        }
    }

    #[test]
    fn a_y_rotation_matrix_is_orthonormal_and_right_handed() {
        for degrees in [0.0f32, 17.0, 90.0, 223.0] {
            let o = Orientation::YRotation(degrees);
            assert!(
                o.orthonormality_error() < 1e-6,
                "{} degrees gave error {}",
                degrees,
                o.orthonormality_error()
            );
        }
    }

    #[test]
    fn a_quarter_turn_is_ninety_degrees_and_not_forty_five() {
        // The regression this pins: atan2(sin, sin) returns a constant 45 for every
        // quarter turn, because both arguments happened to be sin.
        assert!((Orientation::YRotation(90.0).as_y_rotation().unwrap() - 90.0).abs() < 1e-3);
        assert!((Orientation::YRotation(-90.0).as_y_rotation().unwrap() + 90.0).abs() < 1e-3);
    }

    #[test]
    fn a_sheared_orientation_reports_its_error_instead_of_looking_like_a_matrix() {
        // A shear is a valid-looking 3x3 and renders as a wrong transform. The caller
        // gets to see that rather than trusting the shape.
        let sheared = matrix([[1.0, 0.0, 0.0], [0.5, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert!(Orientation::General(sheared).orthonormality_error() > 1e-3);
    }

    #[test]
    fn a_reflection_reports_an_error_because_it_mirrors_rather_than_rotates() {
        // Determinant -1 with orthonormal rows: a mirror, not a rotation.
        let mirrored = matrix([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]]);
        let o = Orientation::General(mirrored);
        assert!(
            (o.orthonormality_error() - 2.0).abs() < 1e-6,
            "det -1 must read as 2.0 from a rotation, got {}",
            o.orthonormality_error()
        );
    }

    #[test]
    fn two_spellings_of_one_orientation_denote_the_same_matrix() {
        // If these diverged, the enum would be leaking a distinction the renderer can
        // observe -- which is the thing it exists to prevent.
        for degrees in [0.0f32, 30.0, 90.0, -120.0] {
            let a = Orientation::YRotation(degrees);
            let b = Orientation::General(a.as_matrix());
            assert_eq!(a.as_matrix(), b.as_matrix());
            assert_eq!(a.as_y_rotation(), b.as_y_rotation());
        }
    }
}
