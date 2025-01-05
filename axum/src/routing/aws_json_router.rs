//! AWS JSON router specified with aws.protocols#awsJson1_1 protocol.

use core::fmt;
use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
};

use axum_core::{
    body::Body,
    extract::Request,
    response::{IntoResponse, Response},
};
use http::{header::CONTENT_TYPE, HeaderValue};
use pin_project_lite::pin_project;
use tower::{Layer, Service};

use crate::handler::Handler;
use crate::routing::HttpBody;

use super::{
    not_found::NotFound, route::RouteFuture, try_downcast, BoxedIntoRoute, Endpoint, Fallback,
    IntoMakeService, MethodRouter, Route,
};

use crate::routing::IntoMakeServiceWithConnectInfo;

/// The router type for composing handlers and services.
#[must_use]
#[derive(Clone)]
pub struct AWSJsonRouter<S = ()> {
    inner: HashMap<&'static str, Endpoint<S>>,
    /// The value of this header is the shape name of the service's Shape ID joined to the shape name of the operation's Shape ID,
    /// separated by a single period (.) character.
    x_amz_target: &'static str,
    /// This header has a static value of `application/x-amz-json-1.1`.
    content_type: &'static str,
    catch_all_fallback: Fallback<S>,
}

impl<S> fmt::Debug for AWSJsonRouter<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AWSJsonRouter")
            .field("router", &self.inner)
            .field("x_amz_target", &self.x_amz_target)
            .field("content_type", &self.content_type)
            .field("catch_all_fallback", &self.catch_all_fallback)
            .finish()
    }
}

impl<S> AWSJsonRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Create a new router.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            x_amz_target: "x-amz-target",
            content_type: "application/x-amz-json-1.1",
            catch_all_fallback: Fallback::Default(Route::new(NotFound)),
        }
    }

    pub(super) fn route_endpoint(self, path: &'static str, endpoint: Endpoint<S>) -> Self {
        let mut inner = self.inner;
        inner.insert(path, endpoint);
        Self {
            inner,
            x_amz_target: self.x_amz_target,
            content_type: self.content_type,
            catch_all_fallback: self.catch_all_fallback,
        }
    }

    /// Add another route to the router.
    #[track_caller]
    pub fn route(self, path: &'static str, method_router: MethodRouter<S>) -> Self {
        self.route_endpoint(path, Endpoint::MethodRouter(method_router))
    }

    /// Add another route to the router that calls a [`Service`].
    pub fn route_service<T>(self, path: &'static str, service: T) -> Self
    where
        T: Service<Request, Error = Infallible> + Clone + Send + Sync + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static,
    {
        let service = match try_downcast::<AWSJsonRouter<S>, _>(service) {
            Ok(_) => {
                panic!(
                    "Invalid route: `AWSJsonRouter::route_service` cannot be used with `AWSJsonRouter`s."
                );
            }
            Err(service) => service,
        };

        self.route_endpoint(path, Endpoint::Route(Route::new(service)))
    }

    /// Apply a [`tower::Layer`] to all routes in the router.
    pub fn layer<L>(self, layer: L) -> AWSJsonRouter<S>
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        let routes = self
            .inner
            .into_iter()
            .map(|(id, endpoint)| {
                let route = endpoint.layer(layer.clone());
                (id, route)
            })
            .collect();

        AWSJsonRouter {
            inner: routes,
            x_amz_target: self.x_amz_target,
            content_type: self.content_type,
            catch_all_fallback: self.catch_all_fallback.map(|route| route.layer(layer)),
        }
    }

    /// Apply a [`tower::Layer`] to the router that will only run if the request matches
    /// a route.
    #[track_caller]
    pub fn route_layer<L>(self, layer: L) -> Self
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        if self.inner.is_empty() {
            panic!(
                "Adding a route_layer before any routes is a no-op. \
             Add the routes you want the layer to apply to first."
            );
        }

        let routes = self
            .inner
            .into_iter()
            .map(|(id, endpoint)| {
                let route = endpoint.layer(layer.clone());
                (id, route)
            })
            .collect();

        AWSJsonRouter {
            inner: routes,
            x_amz_target: self.x_amz_target,
            content_type: self.content_type,
            catch_all_fallback: self.catch_all_fallback,
        }
    }

    /// True if the router currently has at least one route added.
    pub fn has_routes(&self) -> bool {
        !self.inner.is_empty()
    }

    /// Add a fallback [`Handler`] to the router.
    #[track_caller]
    pub fn fallback<H, T>(self, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: 'static,
    {
        Self {
            inner: self.inner,
            x_amz_target: self.x_amz_target,
            content_type: self.content_type,
            catch_all_fallback: Fallback::BoxedHandler(BoxedIntoRoute::from_handler(
                handler.clone(),
            )),
        }
    }

    /// Add a fallback [`Service`] to the router.    
    pub fn fallback_service<T>(self, service: T) -> Self
    where
        T: Service<Request, Error = Infallible> + Clone + Send + Sync + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static,
    {
        Self {
            inner: self.inner,
            x_amz_target: self.x_amz_target,
            content_type: self.content_type,
            catch_all_fallback: Fallback::Service(Route::new(service)),
        }
    }
    /// Provide the state for the router. State passed to this method is global and will be used
    /// for all requests this router receives. That means it is not suitable for holding state derived from a request,
    /// such as authorization data extracted in a middleware. Use [`Extension`] instead for such data.
    pub fn with_state<S2>(self, state: S) -> AWSJsonRouter<S2> {
        let routes = self
            .inner
            .into_iter()
            .map(|(id, endpoint)| {
                let endpoint: Endpoint<S2> = match endpoint {
                    Endpoint::MethodRouter(method_router) => {
                        Endpoint::MethodRouter(method_router.with_state(state.clone()))
                    }
                    Endpoint::Route(route) => Endpoint::Route(route),
                };
                (id, endpoint)
            })
            .collect();

        AWSJsonRouter {
            inner: routes,
            x_amz_target: self.x_amz_target,
            content_type: self.content_type,
            catch_all_fallback: self.catch_all_fallback.with_state(state),
        }
    }

    pub(crate) fn call_with_state(
        &self,
        mut req: Request,
        state: S,
    ) -> AwsContentTypeFuture<Infallible> {
        #[cfg(feature = "original-uri")]
        {
            use crate::extract::OriginalUri;

            if req.extensions().get::<OriginalUri>().is_none() {
                let original_uri = OriginalUri(req.uri().clone());
                req.extensions_mut().insert(original_uri);
            }
        }

        let (parts, body) = req.into_parts();

        if let Some(content_type) = parts.headers.get(http::header::CONTENT_TYPE) {
            if content_type == self.content_type && parts.method == http::Method::POST {
                if let Some(header_action) = parts.headers.get(self.x_amz_target) {
                    if let Ok(action) = header_action.to_str() {
                        if let Some(endpoint) = self.inner.get(action) {
                            let req = Request::from_parts(parts, body);
                            match endpoint {
                                Endpoint::MethodRouter(method_router) => {
                                    return AwsContentTypeFuture::new(
                                        method_router.call_with_state(req, state),
                                        self.content_type,
                                    );
                                }
                                Endpoint::Route(route) => {
                                    return AwsContentTypeFuture::new(
                                        route.clone().call_owned(req),
                                        self.content_type,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        let (req, state) = (Request::from_parts(parts, body), state); // invalid input
        AwsContentTypeFuture::new(
            self.catch_all_fallback.clone().call_with_state(req, state),
            self.content_type,
        )
    }

    /// Convert the router into an owned [`Service`] with a fixed request body type, to aid type
    /// inference.
    pub fn into_service<B>(self) -> AWSJsonRouterIntoService<B, S> {
        AWSJsonRouterIntoService {
            router: self,
            _marker: PhantomData,
        }
    }
}

impl AWSJsonRouter {
    /// Convert this router into a [`MakeService`], that is a [`Service`] whose
    /// response is another service.
    /// [`MakeService`]: tower::make::MakeService
    pub fn into_make_service(self) -> IntoMakeService<Self> {
        // call `Router::with_state` such that everything is turned into `Route` eagerly
        // rather than doing that per request
        IntoMakeService::new(self.with_state(()))
    }

    /// Convert this router into a [`MakeService`], that will store `C`'s
    /// associated `ConnectInfo` in a request extension such that [`ConnectInfo`]
    /// can extract it.
    ///
    /// This enables extracting things like the client's remote address.
    ///
    /// Extracting [`std::net::SocketAddr`] is supported out of the box.
    #[cfg(feature = "tokio")]
    pub fn into_make_service_with_connect_info<C>(self) -> IntoMakeServiceWithConnectInfo<Self, C> {
        // call `AWSJsonRouter::with_state` such that everything is turned into `Route` eagerly
        // rather than doing that per request

        use crate::extract::connect_info::IntoMakeServiceWithConnectInfo;
        IntoMakeServiceWithConnectInfo::new(self.with_state(()))
    }
}

impl<B> Service<Request<B>> for AWSJsonRouter<()>
where
    B: HttpBody<Data = bytes::Bytes> + Send + 'static,
    B::Error: Into<axum_core::BoxError>,
{
    type Response = Response;
    type Error = Infallible;
    type Future = AwsContentTypeFuture<Infallible>;

    #[inline]
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn call(&mut self, req: Request<B>) -> Self::Future {
        let req = req.map(Body::new);
        self.call_with_state(req, ())
    }
}

// for `axum::serve(listener, router)`
#[cfg(all(feature = "tokio", any(feature = "http1", feature = "http2")))]
const _: () = {
    use crate::serve;

    impl<L> Service<serve::IncomingStream<'_, L>> for AWSJsonRouter<()>
    where
        L: serve::Listener,
    {
        type Response = Self;
        type Error = Infallible;
        type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: serve::IncomingStream<'_, L>) -> Self::Future {
            // call `Router::with_state` such that everything is turned into `Route` eagerly
            // rather than doing that per request
            std::future::ready(Ok(self.clone().with_state(())))
        }
    }
};

impl Default for AWSJsonRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// A [`AWSJsonRouter`] converted into an owned [`Service`] with a fixed body type.
///
/// See [`AWSJsonRouter::into_service`] for more details.
pub struct AWSJsonRouterIntoService<B, S = ()> {
    router: AWSJsonRouter<S>,
    _marker: PhantomData<B>,
}

impl<B, S> Clone for AWSJsonRouterIntoService<B, S>
where
    AWSJsonRouter<S>: Clone,
{
    fn clone(&self) -> Self {
        Self {
            router: self.router.clone(),
            _marker: PhantomData,
        }
    }
}
impl<B, S> fmt::Debug for AWSJsonRouterIntoService<B, S>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AWSJsonRouterIntoService")
            .field("router", &self.router)
            .finish()
    }
}

impl<B> Service<Request<B>> for AWSJsonRouterIntoService<B, ()>
where
    B: HttpBody<Data = bytes::Bytes> + Send + 'static,
    B::Error: Into<axum_core::BoxError>,
{
    type Response = Response;
    type Error = Infallible;
    type Future = AwsContentTypeFuture<Infallible>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        <AWSJsonRouter as Service<Request<B>>>::poll_ready(&mut self.router, cx)
    }

    #[inline]
    fn call(&mut self, req: Request<B>) -> Self::Future {
        self.router.call(req)
    }
}

pin_project! {
    pub struct AwsContentTypeFuture<E> {
        #[pin]
        future: RouteFuture<E>,
        content_type: &'static str,    }
}

impl<E> AwsContentTypeFuture<E> {
    fn new(future: RouteFuture<E>, content_type: &'static str) -> Self {
        Self {
            future,
            content_type,
        }
    }
}

impl<E> Future for AwsContentTypeFuture<E> {
    type Output = <RouteFuture<E> as Future>::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut response = ready!(this.future.poll(cx)?);

        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(this.content_type));

        Poll::Ready(Ok(response))
    }
}
