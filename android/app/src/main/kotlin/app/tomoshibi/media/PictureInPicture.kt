package app.tomoshibi.media

import android.app.Activity

/**
 * The call in a small window when the app goes to the background: whatever is
 * on the stage, or the person being talked to (the Apple apps'
 * PictureInPictureHost). CallModel.pictureInPictureSid says which picture.
 *
 * TODO(media): picture-in-picture. On Android the whole activity goes into the
 * small window (Activity.enterPictureInPictureMode, with
 * `android:supportsPictureInPicture` on the activity and
 * PictureInPictureParams' aspect ratio from the picture's shape); the room
 * view then draws only [sid] full size, through a RemoteVideoView like any
 * tile's, and the hub asks for the small window's size. Until then [enter]
 * does nothing and says so.
 */
class PictureInPicture {
    /** The picture the small window would show. */
    var sid: String? = null
        private set

    /** Follows the call's choice of picture. */
    fun follow(sid: String?) {
        this.sid = sid
    }

    /** Whether the small window can be opened here. */
    val available: Boolean get() = false

    /** Opens the small window for [activity]; false where it could not. */
    fun enter(activity: Activity): Boolean = false
}
