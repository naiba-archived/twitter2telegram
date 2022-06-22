DROP TABLE `blacklists`;
CREATE TABLE `blacklists` (
  `id` INTEGER PRIMARY KEY AUTOINCREMENT,
  `user_id` BIGINT UNSIGNED NOT NULL /* 用户(telegram)ID */,
  `twitter_user_id` BIGINT UNSIGNED NOT NULL /* Twitter 用户ID */,
  `twitter_username` VARCHAR(30) NOT NULL /* Twitter 用户名 */,
  `created_at` DATETIME NOT NULL /* 创建时间 */,
  `type` INT NOT NULL /* 1 禁止此人转发 2 屏蔽此人转发 */
);
CREATE UNIQUE INDEX idx_block ON `blacklists`(`type`, `user_id`,`twitter_user_id`);