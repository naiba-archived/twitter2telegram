table! {
    blacklists (id) {
        id -> Nullable<Integer>,
        user_id -> BigInt,
        twitter_user_id -> BigInt,
        twitter_username -> Text,
        created_at -> Timestamp,
        #[sql_name = "type"]
        type_ -> Integer,
    }
}

table! {
    follows (id) {
        id -> Nullable<Integer>,
        user_id -> BigInt,
        twitter_user_id -> BigInt,
        twitter_username -> Text,
        created_at -> Timestamp,
        follow_rt_count -> BigInt,
        block_rt_count -> BigInt,
    }
}

table! {
    users (id) {
        id -> BigInt,
        label -> Text,
        twitter_access_token -> Nullable<Text>,
        twitter_status -> Bool,
        created_at -> Timestamp,
    }
}

allow_tables_to_appear_in_same_query!(
    blacklists,
    follows,
    users,
);
