use crate::errors::NexusV2Error;
use crate::traits::CircularWalStore;

pub struct BackgroundCompactor;

impl BackgroundCompactor {
    pub async fn auto_compact_if_needed<S: CircularWalStore>(store: &mut S) -> Result<(), NexusV2Error> {
        store.compact().await
    }
}
