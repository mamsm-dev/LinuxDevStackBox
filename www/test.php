<?php
// LinuxDevStackBox - Test Page
?>
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Test Page - LinuxDevStackBox</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; background: #f4f4f4; }
        .container { background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }
        .test-section { margin: 20px 0; padding: 15px; border: 1px solid #ddd; border-radius: 4px; }
        .success { background: #e8f5e8; border-color: #4caf50; }
        .error { background: #ffe8e8; border-color: #f44336; }
        .back-link { display: inline-block; margin-bottom: 20px; padding: 8px 16px; background: #007cba; color: white; text-decoration: none; border-radius: 4px; }
        .back-link:hover { background: #005a87; }
    </style>
</head>
<body>
    <div class="container">
        <a href="index.php" class="back-link">← Back to Home</a>
        
        <h1>🧪 LinuxDevStackBox Test Page</h1>
        
        <div class="test-section success">
            <h3>✅ PHP is working!</h3>
            <p>Current time: <?php echo date('Y-m-d H:i:s'); ?></p>
            <p>PHP Version: <?php echo PHP_VERSION; ?></p>
        </div>

        <div class="test-section">
            <h3>📊 PHP Extensions</h3>
            <p>Loaded extensions: <?php echo count(get_loaded_extensions()); ?></p>
            <p>Available extensions: <?php echo implode(', ', array_slice(get_loaded_extensions(), 0, 10)); ?>...</p>
        </div>

        <div class="test-section">
            <h3>🗄️ MySQL Connection Test</h3>
            <?php
            if (extension_loaded('mysqli')) {
                echo '<p class="success">✅ MySQLi extension is loaded</p>';
                
                // Try to connect to MySQL
                $connection = @mysqli_connect('localhost', 'root', '', '', 3306);
                if ($connection) {
                    echo '<p class="success">✅ MySQL connection successful!</p>';
                    mysqli_close($connection);
                } else {
                    echo '<p class="error">❌ MySQL connection failed: ' . mysqli_connect_error() . '</p>';
                }
            } else {
                echo '<p class="error">❌ MySQLi extension not loaded</p>';
            }
            ?>
        </div>

        <div class="test-section">
            <h3>📁 File System Test</h3>
            <?php
            $test_file = 'test_write.txt';
            if (is_writable('.')) {
                file_put_contents($test_file, 'LinuxDevStackBox test file - ' . date('Y-m-d H:i:s'));
                if (file_exists($test_file)) {
                    echo '<p class="success">✅ File write test successful</p>';
                    unlink($test_file);
                } else {
                    echo '<p class="error">❌ File write test failed</p>';
                }
            } else {
                echo '<p class="error">❌ Directory not writable</p>';
            }
            ?>
        </div>
    </div>
</body>
</html>