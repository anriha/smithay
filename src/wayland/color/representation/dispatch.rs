use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, Weak, protocol::wl_surface::WlSurface,
};

use crate::wayland::compositor;
use crate::wayland::{Dispatch2, GlobalDispatch2};

use super::{
    ColorRepresentationHandler, ColorRepresentationSurfaceCachedState,
    ColorRepresentationSurfaceData, wp_color_representation_manager_v1,
    wp_color_representation_surface_v1,
};

impl<D> GlobalDispatch2<wp_color_representation_manager_v1::WpColorRepresentationManagerV1, D> for ()
where
    D: Dispatch<wp_color_representation_manager_v1::WpColorRepresentationManagerV1, ()>
        + Dispatch2<wp_color_representation_surface_v1::WpColorRepresentationSurfaceV1, Weak<WlSurface>>
        + ColorRepresentationHandler
        + 'static,
{
    fn bind(
        &self,
        state: &mut D,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<wp_color_representation_manager_v1::WpColorRepresentationManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        let state = state.color_representation_state();
        let instance = data_init.init(resource, ());

        for alpha_mode in &state.alpha_modes {
            instance.supported_alpha_mode(*alpha_mode);
        }
        for (coefficients, range) in &state.coefficients_and_ranges {
            instance.supported_coefficients_and_ranges(*coefficients, *range);
        }
        instance.done();

        state.known_instances.push(instance);
    }
}

impl<D> Dispatch2<wp_color_representation_manager_v1::WpColorRepresentationManagerV1, D> for ()
where
    D: Dispatch<wp_color_representation_surface_v1::WpColorRepresentationSurfaceV1, Weak<WlSurface>>
        + Dispatch2<wp_color_representation_surface_v1::WpColorRepresentationSurfaceV1, Weak<WlSurface>>
        + ColorRepresentationHandler
        + 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        resource: &wp_color_representation_manager_v1::WpColorRepresentationManagerV1,
        request: wp_color_representation_manager_v1::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        if let wp_color_representation_manager_v1::Request::GetSurface { id, surface } = request {
            compositor::with_states(&surface, |states| {
                let data = states
                    .data_map
                    .get_or_insert_threadsafe(ColorRepresentationSurfaceData::new);

                if data.is_resource_attached() {
                    resource.post_error(
                        wp_color_representation_manager_v1::Error::SurfaceExists,
                        "surface already has a color representation object",
                    );
                    return;
                }

                // TODO: verify the coefficients/range and chroma location against the
                // buffer pixel format on commit, as the protocol asks.
                let instance = data_init.init(id, surface.downgrade());
                *data.instance.lock().unwrap() = Some(instance);
            });
        }
    }

    fn destroyed(
        &self,
        state: &mut D,
        _client: wayland_backend::server::ClientId,
        resource: &wp_color_representation_manager_v1::WpColorRepresentationManagerV1,
    ) {
        state
            .color_representation_state()
            .known_instances
            .retain(|i| i != resource);
    }
}

impl<D> Dispatch2<wp_color_representation_surface_v1::WpColorRepresentationSurfaceV1, D>
    for Weak<WlSurface>
where
    D: ColorRepresentationHandler + 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        resource: &wp_color_representation_surface_v1::WpColorRepresentationSurfaceV1,
        request: wp_color_representation_surface_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_color_representation_surface_v1::Request::SetAlphaMode { alpha_mode } => {
                let wayland_server::WEnum::Value(alpha_mode) = alpha_mode else {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::AlphaMode,
                        "unknown alpha mode",
                    );
                    return;
                };

                if !state.color_representation_state().alpha_modes.contains(&alpha_mode) {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::AlphaMode,
                        "alpha mode not advertised by the compositor",
                    );
                    return;
                }

                let Ok(surface) = self.upgrade() else {
                    return;
                };

                compositor::with_states(&surface, |states| {
                    let mut guard = states
                        .cached_state
                        .get::<Option<ColorRepresentationSurfaceCachedState>>();
                    let representation = guard.pending();
                    representation.get_or_insert_default().alpha_mode = Some(alpha_mode);
                });
            }
            wp_color_representation_surface_v1::Request::SetCoefficientsAndRange {
                coefficients,
                range,
            } => {
                let wayland_server::WEnum::Value(coefficients) = coefficients else {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::Coefficients,
                        "unknown matrix coefficients",
                    );
                    return;
                };
                let wayland_server::WEnum::Value(range) = range else {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::Coefficients,
                        "unknown color range",
                    );
                    return;
                };

                if !state
                    .color_representation_state()
                    .coefficients_and_ranges
                    .contains(&(coefficients, range))
                {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::Coefficients,
                        "coefficients and range not advertised by the compositor",
                    );
                    return;
                }

                let Ok(surface) = self.upgrade() else {
                    return;
                };

                compositor::with_states(&surface, |states| {
                    let mut guard = states
                        .cached_state
                        .get::<Option<ColorRepresentationSurfaceCachedState>>();
                    let representation = guard.pending();
                    representation.get_or_insert_default().coefficients_and_range =
                        Some((coefficients, range));
                });
            }
            wp_color_representation_surface_v1::Request::SetChromaLocation { chroma_location } => {
                let wayland_server::WEnum::Value(chroma_location) = chroma_location else {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::ChromaLocation,
                        "unknown chroma location",
                    );
                    return;
                };

                let Ok(surface) = self.upgrade() else {
                    return;
                };

                compositor::with_states(&surface, |states| {
                    let mut guard = states
                        .cached_state
                        .get::<Option<ColorRepresentationSurfaceCachedState>>();
                    let representation = guard.pending();
                    representation.get_or_insert_default().chroma_location = Some(chroma_location);
                });
            }
            wp_color_representation_surface_v1::Request::Destroy => {
                // Cleanup happens in `destroyed`.
            }
            _ => {}
        }
    }

    fn destroyed(
        &self,
        _state: &mut D,
        _client: wayland_backend::server::ClientId,
        _resource: &wp_color_representation_surface_v1::WpColorRepresentationSurfaceV1,
    ) {
        let Ok(surface) = self.upgrade() else {
            return;
        };

        compositor::with_states(&surface, |states| {
            if let Some(data) = states.data_map.get::<ColorRepresentationSurfaceData>() {
                data.instance.lock().unwrap().take();
            }

            *states
                .cached_state
                .get::<Option<ColorRepresentationSurfaceCachedState>>()
                .pending() = None;
        });
    }
}
