//! The cookies of published READDIR replies: one memory page per reply, holding
//! exactly the names its buffer accepted.
use super::kernel_directory_read::decode_names;
use super::INLINE_PAGES;
use crate::{
    contract::kernel_directory::Published, KernelCookieOffer, KernelDirectoryCursor,
    KernelDirectoryPage, Overlay, OverlayError, OverlayResult, PAGE_ROWS,
};
impl Overlay {
    /// Inside the reading visit's job, for a window that may list a name:
    /// the latest published reply of this open directory listed after the
    /// same name, and a fresh range of `PAGE_ROWS` numbers from the engine's
    /// owner counter. One read; nothing is written, and neither makes an
    /// offset valid.
    ///
    /// A reply at or above the floor listed from the start exists exactly
    /// when the handle has an offset left, so finding one at offset 0 is the
    /// rewind: the offer reuses nothing and its publication retires them.
    pub fn offer_kernel_cookies(
        &self,
        page: &KernelDirectoryPage,
    ) -> OverlayResult<KernelCookieOffer> {
        let directory = page.directory;
        self.available()?;
        let after = page.cursor.after_name().unwrap_or(&[]).to_vec();
        let existing = {
            let state = self.open.borrow();
            let store = state
                .cookies
                .get(&directory.owner)
                .ok_or(OverlayError::Stale)?;
            store
                .after
                .range((after.clone(), page.floor)..=(after.clone(), u64::MAX))
                .next_back()
                .and_then(|((_, first), _)| {
                    store
                        .offsets
                        .get(first)
                        .map(|page| (*first, page.names.clone()))
                })
        };
        let published = match existing {
            None => Published::None,
            Some(_) if page.cursor == KernelDirectoryCursor::Start => Published::Rewind,
            Some((first, names)) => Published::Latest(
                first,
                decode_names(&names).ok_or(OverlayError::Invalid("cookie names"))?,
            ),
        };
        let first = self.mint_owners(PAGE_ROWS as u64)?;
        if first < 3 {
            return Err(OverlayError::Invalid("directory cookie range"));
        }
        Ok(KernelCookieOffer {
            directory,
            after,
            first,
            published,
        })
    }
    /// READDIR's publishing visit, before the reply is sent: the names the
    /// reply buffer accepted become the offsets `first..first+names.len()`
    /// of the offer's fresh range, as one page, in one atomic memory job behind the
    /// fence on the still open descriptor. A closed or released handle, a
    /// revoked mount and a closed Workspace publish nothing. An unavailable
    /// send outcome keeps the row; a second publication of one offer fails
    /// whole on the row's key. Never replay it.
    ///
    /// A rewinding offer first raises the handle's floor to its range, also
    /// when its reply accepted no name. On a handle that has a floor, every
    /// publication deletes at most `INLINE_PAGES` replies below it; what is
    /// left goes with a later publication or with the handle. An offer made
    /// before the floor passed its range is Stale and writes nothing.
    pub fn publish_kernel_cookies(
        &self,
        offer: &KernelCookieOffer,
        names: &[Vec<u8>],
    ) -> OverlayResult<()> {
        let mut blob = Vec::with_capacity(names.iter().map(|name| 1 + name.len()).sum());
        let mut last = offer.after.as_slice();
        for name in names {
            if name.is_empty()
                || name.len() > 255
                || name.as_slice() == b"."
                || name.as_slice() == b".."
                || name.contains(&0)
                || name.contains(&b'/')
                || name.as_slice() <= last
            {
                return Err(OverlayError::Invalid("kernel cookie names"));
            }
            blob.push(name.len() as u8);
            blob.extend_from_slice(name);
            last = name;
        }
        if names.len() > PAGE_ROWS || (names.is_empty() && !offer.rewinds()) {
            return Err(OverlayError::Invalid("kernel cookie names"));
        }
        let directory = offer.directory;
        self.atomic(|| {
            let (_, mut floor) =
                self.directory_fence(directory.mount, directory.serial, directory.owner, true)?;
            if offer.first < floor {
                return Err(OverlayError::Stale);
            }
            if offer.rewinds() {
                floor = offer.first;
                let mut state = self.open.borrow_mut();
                let (mut held, _) = state
                    .directories
                    .get(directory.owner)
                    .ok_or(OverlayError::Stale)?;
                held.floor = floor;
                state.directories.update(directory.owner, held)?;
            }
            if floor != 0 {
                self.open
                    .borrow_mut()
                    .retire_cookies(directory.owner, floor, INLINE_PAGES)?;
            }
            if names.is_empty() {
                return Ok(());
            }
            self.open.borrow_mut().put_cookie(
                directory.owner,
                offer.first,
                crate::open::CookiePage {
                    after: offer.after.clone(),
                    names: blob,
                },
            )?;
            Ok(())
        })
    }
}
