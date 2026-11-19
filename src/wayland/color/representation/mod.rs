//! Implementation of the wp-color-representation-v1 protocol.
//!
//! Clients use this protocol to say how the channels of a buffer are encoded, on top of
//! the colorimetry described by [`wp-color-management-v1`](crate::wayland::color::management):
//! how alpha is premultiplied, which matrix coefficients and range reconstruct RGB from
//! YCbCr, and where sub-sampled chroma samples sit. The metadata is double-buffered
//! surface state; the committed value can be read with [`get_color_representation`].
//!
//! The bindings are generated from the staging XML shipped by `wayland-protocols`, the
//! same one clients compile against, so the wire format cannot drift from the protocol
//! the rest of the ecosystem speaks.
//!
//! ## Usage
//!
//! Implement [`ColorRepresentationHandler`], create a [`ColorRepresentationState`] and
//! route the protocol objects with the crate-wide
//! [`delegate_dispatch2!`](crate::delegate_dispatch2) macro. Compose the state with the
//! sets of values you are willing to accept; the manager advertises them on bind and
//! the requests are validated against them.

use std::sync::Mutex;

use wayland_protocols::wp::color_representation::v1::server::{
    wp_color_representation_manager_v1,
    wp_color_representation_surface_v1::{self, WpColorRepresentationSurfaceV1},
};
use wayland_server::{
    Dispatch, DisplayHandle, GlobalDispatch, Weak, protocol::wl_surface::WlSurface,
};

use crate::wayland::compositor::{self, Cacheable};

pub use wp_color_representation_surface_v1::{AlphaMode, ChromaLocation, Coefficients, Range};

mod dispatch;

/// The matrix coefficients and color range a surface's buffer is encoded with.
///
/// The protocol always sets the two together; they describe one equation, not two
/// independent knobs.
pub type CoefficientsAndRange = (Coefficients, Range);

/// State of the color representation global.
#[derive(Debug)]
pub struct ColorRepresentationState {
    alpha_modes: Vec<AlphaMode>,
    coefficients_and_ranges: Vec<CoefficientsAndRange>,
    known_instances: Vec<wp_color_representation_manager_v1::WpColorRepresentationManagerV1>,
}

/// Handler for the wp-color-representation-v1 protocol.
pub trait ColorRepresentationHandler {
    /// Access the state.
    fn color_representation_state(&mut self) -> &mut ColorRepresentationState;
}

/// Pending and committed color representation of a surface.
#[derive(Debug, Clone, Copy, Default)]
struct ColorRepresentationSurfaceCachedState {
    alpha_mode: Option<AlphaMode>,
    coefficients_and_range: Option<CoefficientsAndRange>,
    chroma_location: Option<ChromaLocation>,
}

impl Cacheable for Option<ColorRepresentationSurfaceCachedState> {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        *self
    }

    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        match (self, into) {
            (Some(this), Some(into)) => {
                into.alpha_mode = this.alpha_mode.or(into.alpha_mode);
                into.coefficients_and_range =
                    this.coefficients_and_range.or(into.coefficients_and_range);
                into.chroma_location = this.chroma_location.or(into.chroma_location);
            }
            (this, into) => {
                *into = this;
            }
        }
    }
}

/// Committed color representation of a surface.
///
/// An unset field means the client set nothing for it, and the compositor is free to
/// choose; the alpha mode defaults to [`AlphaMode::PremultipliedElectrical`] as the
/// protocol requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorRepresentation {
    /// How alpha is premultiplied into the color channels.
    pub alpha_mode: AlphaMode,
    /// Matrix coefficients and range, or `None` if the client set nothing.
    pub coefficients_and_range: Option<CoefficientsAndRange>,
    /// Position of sub-sampled chroma samples, or `None` if the client set nothing.
    pub chroma_location: Option<ChromaLocation>,
}

/// Get the committed color representation of a surface.
pub fn get_color_representation(surface: &WlSurface) -> ColorRepresentation {
    compositor::with_states(surface, |states| {
        states
            .cached_state
            .get::<Option<ColorRepresentationSurfaceCachedState>>()
            .current()
            .as_ref()
            .map(|state| ColorRepresentation {
                alpha_mode: state.alpha_mode.unwrap_or(AlphaMode::PremultipliedElectrical),
                coefficients_and_range: state.coefficients_and_range,
                chroma_location: state.chroma_location,
            })
            .unwrap_or_else(|| ColorRepresentation {
                alpha_mode: AlphaMode::PremultipliedElectrical,
                coefficients_and_range: None,
                chroma_location: None,
            })
    })
}

/// Data attached to a surface that has a color representation object.
#[derive(Debug)]
struct ColorRepresentationSurfaceData {
    instance: Mutex<Option<WpColorRepresentationSurfaceV1>>,
}

impl ColorRepresentationSurfaceData {
    fn new() -> Self {
        Self {
            instance: Mutex::new(None),
        }
    }

    fn is_resource_attached(&self) -> bool {
        self.instance.lock().unwrap().is_some()
    }
}

impl ColorRepresentationState {
    /// Create the global.
    ///
    /// `alpha_modes` and `coefficients_and_ranges` are the values advertised to clients;
    /// requests using anything else are rejected with the corresponding protocol error.
    pub fn new<D>(
        dh: &DisplayHandle,
        alpha_modes: impl IntoIterator<Item = AlphaMode>,
        coefficients_and_ranges: impl IntoIterator<Item = CoefficientsAndRange>,
    ) -> ColorRepresentationState
    where
        D: GlobalDispatch<wp_color_representation_manager_v1::WpColorRepresentationManagerV1, ()>
            + Dispatch<wp_color_representation_manager_v1::WpColorRepresentationManagerV1, ()>
            + Dispatch<WpColorRepresentationSurfaceV1, Weak<WlSurface>>
            + ColorRepresentationHandler
            + 'static,
    {
        dh.create_global::<D, wp_color_representation_manager_v1::WpColorRepresentationManagerV1, ()>(1, ());
        ColorRepresentationState {
            alpha_modes: alpha_modes.into_iter().collect(),
            coefficients_and_ranges: coefficients_and_ranges.into_iter().collect(),
            known_instances: Vec::new(),
        }
    }

    /// Alpha modes this compositor accepts.
    pub fn alpha_modes(&self) -> &[AlphaMode] {
        &self.alpha_modes
    }

    /// Matrix coefficient and range combinations this compositor accepts.
    pub fn coefficients_and_ranges(&self) -> &[CoefficientsAndRange] {
        &self.coefficients_and_ranges
    }
}
