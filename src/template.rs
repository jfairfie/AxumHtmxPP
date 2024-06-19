use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use crate::models::{Point, Todo};

#[derive(Template)]
#[template(path = "index.html")] // Specify the path to the index.html template file
pub struct IndexTemplate {}

#[derive(Template)]
#[template(path = "pointingbuttons.html")]
pub struct PointingButtonsTemplate {
    pub name: String,
    pub id: usize,
    pub points: Vec<Point>,
}

#[derive(Template)]
#[template(path = "todos.html")]
pub struct TodosTemplate {
    pub todos: Vec<Todo>
}

#[derive(Template)]
#[template(path = "pointpage.html")]
pub struct PointPageTemplate {}

#[derive(Template)]
#[template(path = "point.html")]
pub struct PointTemplate {
    pub points: Vec<Point>
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