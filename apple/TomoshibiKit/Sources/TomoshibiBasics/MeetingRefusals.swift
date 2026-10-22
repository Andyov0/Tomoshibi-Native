import Foundation

/// The sentence for each code the deployment refuses an arrangement with
/// (Schedule.tsx `explainArranging`, code for code and nothing more), said
/// by the lobby's schedule and by the Messages extension alike: one asks
/// through the core and the other without it, and both are refused with the
/// same codes. The server sends the code and the client owns the words
/// (apple/i18n/README.md).
public enum MeetingRefusals {
    public static func sentence(_ code: String) -> String {
        switch code {
        case "room_already_arranged": t("That room already has a meeting arranged.")
        case "bad_time": t("That time is not one a meeting can be arranged for.")
        case "meetings_need_invites": t("Meetings here ask for an account, so a link cannot let anybody in.")
        case "not_yours": t("That is somebody else's room.")
        case "too_many_meetings": t("You have as many meetings arranged as you can have.")
        case "invalid_room": t("Room names can only use lowercase letters, numbers and dashes.")
        case "scoped_room": t("A room held under a scope cannot be arranged ahead. Send a link with a time on it instead.")
        case "relay_not_allowed": t("Access denied. That server is for administrators.")
        case "not_signed_in": t("Your session has expired. Sign in again.")
        case "not_allowed": t("Arranging meetings has not been opened to you.")
        default: t("Something went wrong. Try again.")
        }
    }
}
