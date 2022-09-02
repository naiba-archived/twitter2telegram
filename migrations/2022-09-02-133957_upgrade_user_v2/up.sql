ALTER TABLE `users` ADD COLUMN `disable_retweet` BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE `users` ADD COLUMN `disable_text_msg` BOOLEAN NOT NULL DEFAULT false;
