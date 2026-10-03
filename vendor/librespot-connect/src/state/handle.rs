use crate::{
    core::{Error, dealer::protocol::SetQueueCommand},
    state::{
        ConnectState,
        context::{ContextType, ResetContext},
    },
};
use protobuf::MessageField;

impl ConnectState {
    pub fn handle_shuffle(&mut self, shuffle: bool) -> Result<(), Error> {
        self.set_shuffle(shuffle);

        if shuffle {
            return self.shuffle_new();
        }

        self.reset_context(ResetContext::DefaultIndex);

        // spotify-tui: put the tracks back in their original order. Upstream
        // only reset the index, so after turning shuffle off the rest of the
        // playlist still played shuffled.
        if let Ok(ctx) = self.get_context_mut(ContextType::Default) {
            ctx.tracks.unshuffle();
        }

        if self.current_track(MessageField::is_none) {
            return Ok(());
        }

        // spotify-tui: look the current track up by uri; its stored context
        // index is its position in the shuffled order
        let ctx = self.get_context(ContextType::Default)?;
        let current_index =
            ConnectState::find_index_in_context(ctx, |c| self.current_track(|t| c.uri == t.uri))?;
        self.reset_playback_to_position(Some(current_index))
    }

    pub fn handle_set_queue(&mut self, set_queue: SetQueueCommand) {
        self.set_next_tracks(set_queue.next_tracks);
        self.set_prev_tracks(set_queue.prev_tracks);
        self.update_queue_revision();
    }

    pub fn handle_set_repeat_context(&mut self, repeat: bool) -> Result<(), Error> {
        self.set_repeat_context(repeat);

        if repeat {
            if let ContextType::Autoplay = self.fill_up_context {
                self.fill_up_context = ContextType::Default;
            }
        }

        let ctx = self.get_context(ContextType::Default)?;
        let current_track =
            ConnectState::find_index_in_context(ctx, |t| self.current_track(|t| &t.uri) == &t.uri)?;
        self.reset_playback_to_position(Some(current_track))
    }
}
