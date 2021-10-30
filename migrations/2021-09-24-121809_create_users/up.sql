CREATE TABLE `users` (
  `id` BIGINT UNSIGNED NOT NULL PRIMARY KEY /* TG用户ID */,
  `label` VARCHAR(8) NOT NULL /* 备注名 */,
  `twitter_access_token` VARCHAR(250),
  `twitter_status` BOOLEAN NOT NULL,
  `created_at` DATETIME NOT NULL /* 创建时间 */
);
