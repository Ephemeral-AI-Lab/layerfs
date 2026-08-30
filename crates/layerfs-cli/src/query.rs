#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewQuery(pub layerfs_sdk::Query);

impl From<ViewQuery> for layerfs_sdk::Query {
    fn from(value: ViewQuery) -> Self {
        value.0
    }
}

#[derive(Clone, Debug)]
pub struct ViewSnapshot(pub layerfs_sdk::QueryPage);
