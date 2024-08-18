use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use crate::models::{Point, Room};

#[derive(Template)]
#[template(path = "pointing.html")]
pub struct PointingPageTemplate {
    pub id: String,
    pub point: String,
    pub room_id: u32,
    pub room_name: String,
}

#[derive(Template)]
#[template(path = "createroom.html")]
pub struct CreateRoomTemplate { }

#[derive(Template)]
#[template(path = "rooms.html")]
pub struct RoomTemplate {
    pub rooms: Vec<Room>,
}

//a wrapper for turning askama templates into responses that can be handled by server
pub struct HtmlTemplate<T>(pub T);

impl<T> IntoResponse for HtmlTemplate<T>
    where
        T: Template,
{
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(), // If rendering is successful, return an HTML response
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR, // If rendering fails, return an internal server error
                format!("Failed to render template. Error: {}", err),
            )
                .into_response(),
        }
    }
}