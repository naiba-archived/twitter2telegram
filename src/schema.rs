table! {
    follows (id) {
        id -> Nullable<Integer>,
        user_id -> BigInt,
        twitter_user_id -> BigInt,
        twitter_username -> Text,
        created_at -> Timestamp,
    }
}

table! {
    users (id) {
        id -> BigInt,
        label -> Text,
        telegram_status -> Bool,
        twitter_access_token -> Nullable<Text>,
        twitter_status -> Bool,
        created_at -> Timestamp,
    }
}

allow_tables_to_appear_in_same_query!(
    follows,
    users,
);
