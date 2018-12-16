#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlogItem {
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PostItem {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrailItem {
    pub blog: BlogItem,
    pub post: PostItem,
    pub content_raw: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Photo {
    pub url: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Photos {
    pub caption: String,
    pub original_size: Photo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Post {
    pub blog_name: String,
    pub id: u64,
    pub post_url: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub timestamp: u64,
    pub date: String,
    pub format: String,
    pub reblog_key: String,
    pub tags: Vec<String>,
    pub body: Option<String>,
    pub trail: Option<Vec<TrailItem>>,
    pub photos: Option<Vec<Photos>>,
    pub video_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryParams {
    pub limit: String,
    pub before: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinksNext {
    pub query_params: QueryParams,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Links {
    pub next: LinksNext,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub liked_count: i32,
    pub liked_posts: Vec<Post>,
    pub _links: Option<Links>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReturnVal {
    pub response: Response,
}
